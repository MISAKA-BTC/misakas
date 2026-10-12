//! **`qwen25_a16_backend`'s `held_real_row_probe` module**, moved out of `qwen25_a16_backend.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

use super::*;
use kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3, fp_job_id_v3,
};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
use kaspa_consensus_core::palw_state_chunk_map::{PALW_HELD_STEP_LADDER_V1, palw_profile_is_held_v4};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, canonical_step_leaf_index};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// The drill both tests run. `backend` builds one instance of the class at the network's
/// ladder — called twice, for the honest party and for the liar.
fn drill(
    label: &str,
    artifact: &std::sync::Arc<Base0ArtifactV1>,
    profile: &PalwShapeProfileV3,
    positions: u32,
    decode: u32,
    network: u64,
    backend: &dyn Fn() -> Qwen25A16Backend,
) {
    let started = std::time::Instant::now();
    assert!(palw_profile_is_held_v4(profile));
    let honest = backend();
    assert_eq!((honest.step_ladder_cap(), honest.materialize_cap()), (PALW_HELD_STEP_LADDER_V1, network));
    let form = honest.prompt_ids_form();
    assert_eq!(form, PalwPromptIdsFormV1::MerkleV1, "a held class commits its own Merkle ids");
    let vocab = artifact.shape.vocab;
    let prompt: Vec<usize> = (0..positions as usize).map(|i| (i * 7919 + 1013) % vocab).collect();
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, &ids).expect("the ids commit"),
        prompt_tokens: positions,
        decode_token_limit: decode,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };

    // ---- the producer: the fold, past the network's ladder ----------------------------
    let t = std::time::Instant::now();
    let run = honest.execute_free_prompt(&job, &prompt).expect("the held producer runs past the network's ladder");
    let produced = t.elapsed();
    let leaves = run.facts.step_leaf_count;
    assert!(leaves > network, "{leaves} leaves: the job must be past the network's ladder of {network} to measure anything");
    let capture = run.outcome.material.clone();
    let output_ids = run.output_token_ids.clone();
    let claim = PalwClaimRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        anchor: fp_job_id_v3(&job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let count = honest.fp_interval_count(&capture).expect("a held capture has intervals");
    assert_eq!(Some(count), honest.fp_interval_count_for(positions, decode), "the executor's count is the chain's");
    assert!(count >= 3, "the drill needs a first, a middle and a last interval");
    eprintln!(
        "{label}: produced {positions}+{decode} positions, {leaves} leaves, material {} bytes, {count} intervals ({produced:.0?})",
        capture.len()
    );

    // ---- the seat, on the first, middle and last interval -----------------------------
    let mut sampled = vec![0, count / 2, count - 1];
    sampled.dedup();
    for &index in &sampled {
        let t = std::time::Instant::now();
        let opening = honest.open_fp_interval(&capture, index, &ids).unwrap_or_else(|e| panic!("interval {index} opens: {e}"));
        let opened = t.elapsed();
        let v4 = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).expect("a V4 opening");
        let t = std::time::Instant::now();
        if let Some(anchor) = v4.anchor.as_ref() {
            let root = honest
                .checkpoint_root_for_context_v1(&v4.binding.job_context, &ids, &output_ids, anchor.leaf.covered_decode_call)
                .expect("the seat recomputes the anchor");
            assert_eq!(root, anchor.leaf.state_chunks_root, "interval {index}: the seat's state is the committed checkpoint");
        }
        let recomputed = t.elapsed();
        let t = std::time::Instant::now();
        let verdict = honest.verify_fp_interval_opening(&opening, claim, index, &ids, leaves);
        let verified = t.elapsed();
        assert_eq!(verdict, PalwFpIntervalVerdictV1::Valid, "interval {index} of an honest held capture");
        eprintln!(
            "{label}: interval {index}: opening {} bytes (open {opened:.0?}), anchor recompute {recomputed:.0?}, Valid ({verified:.0?})",
            opening.len()
        );
    }

    // ---- a liar: one wrong tile inside the middle interval ------------------------------
    // Inside: a block that straddles an interval's edge is whole in neither interval, so no
    // block answer serves it and no seat names a leaf in it (ADR-0086 Decision 6 as built; the
    // gap is ADR-0121 §7's). The drill lies where the naming route runs.
    let last = count / 2;
    let binding = crate::produce::base0_material_decode_any_v1(&capture).expect("the capture decodes").binding().clone();
    let geometry = crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_capped_v1(
        &binding,
        honest.checkpoint_interval(),
        PALW_HELD_STEP_LADDER_V1,
    )
    .expect("the held geometry");
    let window = geometry.window_for(last).expect("the middle interval's window");
    let step = window.first_step + (window.last_step - window.first_step) / 2;
    let (call, position) = crate::fp_interval::Base0FpWindowV1::coordinate_of_step_v1(positions, step).expect("a step of the job");
    let slot = (0..profile.global_node_count())
        .find(|slot| {
            profile.resolve_node_slot(*slot).is_some_and(|(node, layer)| {
                layer == Some(1)
                    && node.op_kind != PalwStepOpKindV1::AttnFused
                    && !node.input_refs.iter().any(|r| {
                        *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_K
                            || *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_V
                    })
            })
        })
        .expect("layer 1 has a node that reads no cache");
    let leaf = canonical_step_leaf_index(
        profile,
        &binding.job_context,
        &PalwStepCoordinateV1 { call_index: call, node_slot: slot, position, tile_index: 0 },
    )
    .expect("the lie's leaf");
    let liar = backend();
    let t = std::time::Instant::now();
    let lying = liar.execute_free_prompt_with_injected_fault(&job, &prompt, leaf).expect("the drill's liar runs");
    let lied = t.elapsed();
    let lying_capture = lying.outcome.material.clone();
    let lying_claim = PalwClaimRootsV1 {
        execution_root: lying.outcome.execution_root,
        trace_root: lying.outcome.trace_root,
        anchor: fp_job_id_v3(&job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    assert_ne!(lying_claim.execution_root, claim.execution_root, "the lie is committed");
    let t = std::time::Instant::now();
    let opening = liar.open_fp_interval(&lying_capture, last, &ids).expect("the liar serves its interval");
    let served_in = t.elapsed();
    let v4 = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).expect("V4");
    if let Some(anchor) = v4.anchor.as_ref() {
        let root = honest
            .checkpoint_root_for_context_v1(&v4.binding.job_context, &ids, &lying.output_token_ids, anchor.leaf.covered_decode_call)
            .expect("the seat recomputes the liar's anchor");
        assert_eq!(root, anchor.leaf.state_chunks_root, "the liar's checkpoints are honest: only a tile lies");
    }
    let t = std::time::Instant::now();
    let verdict = honest.verify_fp_interval_opening(&opening, lying_claim, last, &ids, leaves);
    let judged = t.elapsed();
    let named = match verdict {
        PalwFpIntervalVerdictV1::Fault { leaf_index } => leaf_index,
        PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, leaf_count } => {
            assert!((first_leaf_index..first_leaf_index + leaf_count).contains(&leaf), "the block holds the lie");
            let block = first_leaf_index >> v4.range.retain_level;
            let number = crate::fp_interval::base0_fp_block_request_number_v1(&v4, block).expect("a numbered block");
            let t = std::time::Instant::now();
            let served = liar.open_fp_block_leaves(&lying_capture, last, number, &ids).expect("the liar serves the block");
            let served_took = t.elapsed();
            let t = std::time::Instant::now();
            let named = honest
                .fp_name_the_leaf_v1(&opening, &served, lying_claim, last, &ids, &lying.output_token_ids, leaves)
                .expect("the block names a leaf")
                .expect("a leaf differs");
            eprintln!(
                "{label}: block {block} (number {number}, {} bytes, served {served_took:.0?}) names leaf {named} ({:.0?})",
                served.len(),
                t.elapsed()
            );
            named
        }
        other => panic!("the seat's verdict on the liar's interval {last} is {other:?}"),
    };
    assert_eq!(named, leaf, "the seat names the leaf the drill lied at");
    let t = std::time::Instant::now();
    let evidence = kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
        &liar,
        &lying_capture,
        &ids,
        lying_claim,
        leaves,
        leaf,
        form,
    )
    .expect("the liar's own fold builds its leaf's evidence");
    let built = t.elapsed();
    let artifact_root = crate::inventory::a16_inventory_v1(artifact, profile).expect("the inventory").root();
    let one_move = evidence.verdict_v1(profile.shape_profile_id(), artifact_root, PALW_HELD_STEP_LADDER_V1);
    eprintln!(
        "{label}: liar at leaf {leaf} (call {call}, position {position}, slot {slot}): produced {lied:.0?}, served interval {last} \
             ({} bytes, {served_in:.0?}), seat {verdict:?} ({judged:.0?}), evidence {} bytes ({built:.0?}), one-move {one_move:?}",
        opening.len(),
        kaspa_consensus_core::palw_shard_court_v1::palw_leaf_evidence_bytes_v1(&evidence)
    );
    assert_eq!(
        one_move,
        Ok(kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::ExecutorGuilty),
        "the court convicts the liar on its own fold's evidence"
    );
    eprintln!("{label}: total {:.0?}", started.elapsed());
}

/// The fixture class: a held row (graph-v7) small enough for the default suite, whose jobs of a
/// few dozen positions are past a `2^12` network ladder.
fn fixture() -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    let geometry = PalwQwen25GeometryV1 {
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
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("a held graph-v7 profile");
    let shape = Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: geometry.rms_eps_q,
    };
    let artifact = std::sync::Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    );
    (artifact, profile)
}

const FIXTURE_NETWORK: u64 = 1 << 12;

fn fixture_backend(artifact: &std::sync::Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3) -> Qwen25A16Backend {
    Qwen25A16Backend::new(artifact.clone(), b"misaka-palw-rc".to_vec(), profile.clone(), (64, 8))
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(FIXTURE_NETWORK)
        .with_prompt_ids_form(PalwPromptIdsFormV1::Flat)
}

/// The drill at fixture scale, in the default suite: a network ladder of `2^12`, which the
/// fixture's job is past, so every route above runs where only the class's ladder admits it.
#[test]
fn a_held_liar_past_the_networks_ladder_is_named_and_convicted() {
    let (artifact, profile) = fixture();
    drill("held-fixture", &artifact, &profile, 64, 8, FIXTURE_NETWORK, &|| fixture_backend(&artifact, &profile));
}

/// **t12's bound one-move verdict on the executor's own evidence** (`palw_audit_2026_09_23`).
/// At a FUSED leaf of a held class the builder's object carries the site's history, which the
/// bound verdict refuses; the filer's strip makes it the object the chain takes
/// (`NeedsDissection`), smaller than the builder's. Below the fence both read v1's
/// `NeedsDissection`. At a non-fused leaf the bound verdict is v1's.
#[test]
fn the_executors_own_fused_evidence_stripped_is_what_the_bound_verdict_takes() {
    use kaspa_consensus_core::palw_shard_court_v1::{
        PalwOneMoveClaimV2, PalwShardCourtError, PalwShardCourtVerdictV1, palw_leaf_evidence_bytes_v1,
    };
    let (artifact, profile) = fixture();
    let backend = fixture_backend(&artifact, &profile);
    let form = backend.prompt_ids_form();
    let vocab = artifact.shape.vocab;
    let positions = 24u32;
    let prompt: Vec<usize> = (0..positions as usize).map(|i| (i * 7919 + 1013) % vocab).collect();
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, &ids).expect("the ids commit"),
        prompt_tokens: positions,
        decode_token_limit: 4,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };
    let run = backend.execute_free_prompt(&job, &prompt).expect("the held producer runs");
    let leaves = run.facts.step_leaf_count;
    let capture = run.outcome.material.clone();
    let claim = PalwClaimRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        anchor: fp_job_id_v3(&job),
        attempt_draw: None,
        // SEAT-S2's field, `None` as SEAT-S2 wrote base0's fixtures: leaf evidence never reads the answer.
        output_root: None,
        job_pin: None,
    };
    let binding = crate::produce::base0_material_decode_any_v1(&capture).expect("decodes").binding().clone();
    let fused_at = |leaf: u64| {
        kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &binding.job_context, leaf).map(|c| {
            (c.position, profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused))
        })
    };
    let fused_leaf = (0..leaves).find(|l| fused_at(*l).is_some_and(|(p, f)| f && p >= 12)).expect("a fused leaf past position 12");
    let plain_leaf =
        (0..fused_leaf).rev().find(|l| fused_at(*l).is_some_and(|(_, f)| !f)).expect("a leaf before it that is not fused");
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let bound_to =
        PalwOneMoveClaimV2 { execution_root: claim.execution_root, class_id: profile.shape_profile_id(), artifact_root: root };
    let ladder = PALW_HELD_STEP_LADDER_V1;
    let build = |leaf: u64| {
        kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
            &backend, &capture, &ids, claim, leaves, leaf, form,
        )
        .expect("the executor builds its leaf's evidence")
    };

    let fused = build(fused_leaf);
    assert!(!fused.refutation.inputs.is_empty(), "the builder carries the site's canonical rows");
    assert_eq!(fused.verdict_v1(bound_to.class_id, root, ladder), Ok(PalwShardCourtVerdictV1::NeedsDissection));
    assert_eq!(fused.verdict_at_v2(&bound_to, ladder, false), Ok(PalwShardCourtVerdictV1::NeedsDissection));
    assert!(
        matches!(fused.verdict_at_v2(&bound_to, ladder, true), Err(PalwShardCourtError::FusedLeafCarriesTheHistory { .. })),
        "{:?}",
        fused.verdict_at_v2(&bound_to, ladder, true)
    );
    let stripped = fused.clone().for_the_one_move_v2();
    assert_eq!(stripped.verdict_at_v2(&bound_to, ladder, true), Ok(PalwShardCourtVerdictV1::NeedsDissection));
    let (full, lean) = (palw_leaf_evidence_bytes_v1(&fused), palw_leaf_evidence_bytes_v1(&stripped));
    eprintln!("fused leaf {fused_leaf}: builder {full} B, stripped {lean} B");
    assert!(lean < full && lean <= kaspa_consensus_core::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES);

    // The non-fused leaves before it, whatever each reads: the bound verdict is v1's, exactly.
    let mut cleared = 0;
    for leaf in (0..plain_leaf + 1).rev().filter(|l| fused_at(*l).is_some_and(|(_, f)| !f)).take(48) {
        let plain = build(leaf);
        assert_eq!(plain.clone().for_the_one_move_v2(), plain, "leaf {leaf}: a non-fused leaf's evidence is untouched");
        let v1 = plain.verdict_v1(bound_to.class_id, root, ladder);
        assert_eq!(plain.verdict_at_v2(&bound_to, ladder, true), v1, "leaf {leaf}: the non-fused verdict is v1's");
        cleared += usize::from(v1 == Ok(PalwShardCourtVerdictV1::FalseAccusation));
    }
    assert!(cleared > 0, "some honest non-fused leaf clears, so the parity is not only over refusals");
}

/// **The fused-leaf sweep** (the shard-court addendum to 8be0f661; ADR-0152 §4-ter): the bound
/// verdict on the executor's own evidence at EVERY fused leaf of a held job — both layers, every
/// prefill position (the first, whose history is one row, included), every decode call, every
/// output tile — not one sampled leaf. At each: the builder's object is refused only for the
/// history it carries; stripped (what a seat files) it is `NeedsDissection`, never a conviction
/// and never a refusal of an honest executor's own tile; below the fence v1 reads
/// `NeedsDissection` on both; and the stripped object is under the close ceiling. A fused leaf
/// the sweep found convicting, or refusing, honest evidence would be one where a seat's
/// accusation of an 8k claim convicts the honest producer in one move or never reaches its
/// dissection.
#[test]
fn every_fused_leaf_of_a_held_job_defers_to_its_dissection_on_honest_evidence() {
    use kaspa_consensus_core::palw_shard_court_v1::{
        PalwOneMoveClaimV2, PalwShardCourtError, PalwShardCourtVerdictV1, palw_leaf_evidence_bytes_v1,
    };
    let (artifact, profile) = fixture();
    let backend = fixture_backend(&artifact, &profile);
    let form = backend.prompt_ids_form();
    let vocab = artifact.shape.vocab;
    let positions = 12u32;
    let decode = 3u32;
    let prompt: Vec<usize> = (0..positions as usize).map(|i| (i * 7919 + 1013) % vocab).collect();
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5B; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, &ids).expect("the ids commit"),
        prompt_tokens: positions,
        decode_token_limit: decode,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };
    let run = backend.execute_free_prompt(&job, &prompt).expect("the held producer runs");
    let leaves = run.facts.step_leaf_count;
    let capture = run.outcome.material.clone();
    let claim = PalwClaimRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        anchor: fp_job_id_v3(&job),
        attempt_draw: None,
        // SEAT-S2's field, `None` as SEAT-S2 wrote base0's fixtures: leaf evidence never reads the answer.
        output_root: None,
        job_pin: None,
    };
    let binding = crate::produce::base0_material_decode_any_v1(&capture).expect("decodes").binding().clone();
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let bound_to =
        PalwOneMoveClaimV2 { execution_root: claim.execution_root, class_id: profile.shape_profile_id(), artifact_root: root };
    let ladder = PALW_HELD_STEP_LADDER_V1;
    let fused: Vec<(u64, PalwStepCoordinateV1)> = (0..leaves)
        .filter_map(|leaf| {
            kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &binding.job_context, leaf).map(|c| (leaf, c))
        })
        .filter(|(_, c)| profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused))
        .collect();
    let (mut layers, mut prefill_positions, mut decode_calls, mut tiles) = (
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    );
    let mut largest = 0u64;
    for (leaf, coord) in &fused {
        let evidence = kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
            &backend, &capture, &ids, claim, leaves, *leaf, form,
        )
        .unwrap_or_else(|e| panic!("leaf {leaf} {coord:?}: the executor builds its own evidence: {e}"));
        assert_eq!(evidence.verdict_at_v2(&bound_to, ladder, false), Ok(PalwShardCourtVerdictV1::NeedsDissection), "leaf {leaf}");
        assert_eq!(evidence.verdict_v1(bound_to.class_id, root, ladder), Ok(PalwShardCourtVerdictV1::NeedsDissection));
        let carried = !evidence.refutation.inputs.is_empty() || evidence.refutation.kv_checkpoint.is_some();
        match evidence.verdict_at_v2(&bound_to, ladder, true) {
            Err(PalwShardCourtError::FusedLeafCarriesTheHistory { .. }) => assert!(carried, "leaf {leaf}"),
            Ok(PalwShardCourtVerdictV1::NeedsDissection) => assert!(!carried, "leaf {leaf}"),
            other => panic!("leaf {leaf} {coord:?}: the builder's own object reads {other:?}"),
        }
        let stripped = evidence.for_the_one_move_v2();
        assert_eq!(
            stripped.verdict_at_v2(&bound_to, ladder, true),
            Ok(PalwShardCourtVerdictV1::NeedsDissection),
            "leaf {leaf} {coord:?}: the seat's object defers to the dissection"
        );
        let bytes = palw_leaf_evidence_bytes_v1(&stripped);
        assert!(bytes <= kaspa_consensus_core::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES, "leaf {leaf}: {bytes} B");
        largest = largest.max(bytes);
        let (_, layer) = profile.resolve_node_slot(coord.node_slot).expect("the node");
        layers.insert(layer);
        if coord.call_index == 0 {
            prefill_positions.insert(coord.position);
        } else {
            decode_calls.insert(coord.call_index);
        }
        tiles.insert(coord.tile_index);
    }
    eprintln!(
        "{} fused leaves swept of {leaves}: layers {layers:?}, {} prefill positions, decode calls {decode_calls:?}, tiles {tiles:?}, \
             largest stripped object {largest} B",
        fused.len(),
        prefill_positions.len()
    );
    assert_eq!(layers.len(), profile.layer_count as usize, "every layer's fused site");
    assert_eq!(prefill_positions.len(), positions as usize, "every prefill position, the first included");
    let calls = binding.job_context.exact_decode_tokens.saturating_sub(1) as usize;
    assert!(calls >= 1, "the job decodes past its first token, so decode positions are swept too");
    assert_eq!(decode_calls.len(), calls, "every decode call");
    assert!(tiles.len() > 1, "every output tile of the site, not the first alone");
}

/// **ADR-0121 §7: a lie in a block that straddles an interval's edge is named from the edges.**
/// Such a block is whole in neither interval, so no opening digests it and the block naming
/// refuses it; the edges are served as their leaves and checked by the range's root walk. The
/// fault is addressed at the left edge whenever the range starts off a block boundary — the
/// wrong edge here, where the lie is on the right — and that edge alone does not walk; the right
/// one names the leaf, and so do both; an honest executor's edges name nothing.
#[test]
fn a_lie_in_a_block_that_straddles_an_intervals_edge_is_named_from_the_edges() {
    let (artifact, profile) = fixture();
    let honest = fixture_backend(&artifact, &profile);
    let form = honest.prompt_ids_form();
    let vocab = artifact.shape.vocab;
    let prompt: Vec<usize> = (0..64usize).map(|i| (i * 7919 + 1013) % vocab).collect();
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, &ids).expect("the ids commit"),
        prompt_tokens: 64,
        decode_token_limit: 8,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };
    let run = honest.execute_free_prompt(&job, &prompt).expect("the held producer runs");
    let (capture, leaves) = (run.outcome.material.clone(), run.facts.step_leaf_count);
    let claim = PalwClaimRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        anchor: fp_job_id_v3(&job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let mid = honest.fp_interval_count(&capture).expect("intervals") / 2;
    let opening = honest.open_fp_interval(&capture, mid, &ids).expect("the interval opens");
    let v4 = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).expect("V4");
    let anchor = v4.anchor.as_ref().expect("the middle interval resumes from a checkpoint").leaf.covered_decode_call;
    honest.checkpoint_root_for_context_v1(&v4.binding.job_context, &ids, &run.output_token_ids, anchor).expect("the seat recomputes");
    let (left, right) = v4.range.edges_v1(leaves);
    assert!(left.0 < left.1 && right.0 < right.1, "the middle interval's range starts and ends inside a block: {left:?} {right:?}");
    let r = v4.range.retain_level;
    let number = |block: u64| crate::fp_interval::base0_fp_block_request_number_v1(&v4, block).expect("a numbered block");
    let (left_block, right_block) = (left.0 >> r, right.0 >> r);

    // The honest executor's edges walk, and name nothing.
    let honest_edges = vec![
        honest.open_fp_block_leaves(&capture, mid, number(left_block), &ids).expect("the left edge is served"),
        honest.open_fp_block_leaves(&capture, mid, number(right_block), &ids).expect("the right edge is served"),
    ];
    assert_eq!(honest.fp_name_the_edge_leaf_v1(&opening, &honest_edges, claim, mid, &ids, &run.output_token_ids, leaves), Ok(None));

    // A liar on the right edge.
    let leaf = right.0 + (right.1 - right.0) / 2;
    let liar = fixture_backend(&artifact, &profile);
    let lying = liar.execute_free_prompt_with_injected_fault(&job, &prompt, leaf).expect("the drill's liar runs");
    let lying_capture = lying.outcome.material.clone();
    let lying_claim = PalwClaimRootsV1 {
        execution_root: lying.outcome.execution_root,
        trace_root: lying.outcome.trace_root,
        anchor: fp_job_id_v3(&job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let lie = liar.open_fp_interval(&lying_capture, mid, &ids).expect("the liar serves its interval");
    let verdict = honest.verify_fp_interval_opening(&lie, lying_claim, mid, &ids, leaves);
    assert_eq!(
        verdict,
        PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index: left.0, leaf_count: left.1 - left.0 },
        "every digest agrees and the root does not: the fault is addressed at the left edge"
    );
    let served_left = liar.open_fp_block_leaves(&lying_capture, mid, number(left_block), &ids).expect("the left edge");
    let served_right = liar.open_fp_block_leaves(&lying_capture, mid, number(right_block), &ids).expect("the right edge");
    assert!(
        honest.fp_name_the_leaf_v1(&lie, &served_right, lying_claim, mid, &ids, &lying.output_token_ids, leaves).is_err(),
        "the block naming refuses an edge: no digest to check it against"
    );
    assert!(
        honest
            .fp_name_the_edge_leaf_v1(
                &lie,
                std::slice::from_ref(&served_left),
                lying_claim,
                mid,
                &ids,
                &lying.output_token_ids,
                leaves
            )
            .is_err(),
        "the addressed edge alone does not walk: the lie is in the edge still the seat's own"
    );
    for edges in [vec![served_right.clone()], vec![served_left, served_right]] {
        assert_eq!(
            honest.fp_name_the_edge_leaf_v1(&lie, &edges, lying_claim, mid, &ids, &lying.output_token_ids, leaves),
            Ok(Some(leaf)),
            "the served edges walk to the committed root and name the lie"
        );
    }
}

#[test]
fn a_held_job_past_the_networks_ladder_on_the_real_dense_row() {
    let Ok(path) = std::env::var("MISAKA_PALW_HELD_ARTIFACT") else {
        eprintln!("held-real: skipped — set MISAKA_PALW_HELD_ARTIFACT to the dense .palwart to measure");
        return;
    };
    let positions = env_u32("MISAKA_PALW_HELD_POSITIONS", 1024);
    let decode = env_u32("MISAKA_PALW_HELD_DECODE", 4);
    let n_ctx = (positions + decode).next_power_of_two();
    let started = std::time::Instant::now();
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut artifact = crate::artifact::decode_artifact_file_v1(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    drop(bytes);
    eprintln!("held-real: artifact loaded ({:.0?})", started.elapsed());
    // **The artifact's RoPE table is a width of its own.** The shipped 1.5B artifact was converted
    // at the 512-position row, and its inventory — the class's registered root — commits a rotary
    // row for each of those positions and none past them, so a held class over THAT artifact
    // cannot run, or be adjudicated, past 512 positions whatever its ladder. A held class is
    // registered over an artifact converted at its own width; this probe extends the table with
    // the same generator (its first rows are checked to be the artifact's own) and measures the
    // node's routes over that artifact.
    let converted_at = artifact.shape.max_position;
    if converted_at < n_ctx as usize {
        let extended = crate::rope::RopeTableV1::generate(artifact.shape.d_head, n_ctx as usize, artifact.shape.ln_theta_gen_q)
            .expect("the rotary generator extends");
        let committed = artifact.rope.cos_q.len();
        assert_eq!(
            (&extended.cos_q[..committed], &extended.sin_q[..committed]),
            (&artifact.rope.cos_q[..], &artifact.rope.sin_q[..]),
            "the generator's first {converted_at} rows are the artifact's own"
        );
        artifact.shape.max_position = n_ctx as usize;
        artifact.rope = extended;
        eprintln!("held-real: the artifact's RoPE table stops at {converted_at} positions — extended to {n_ctx} for the probe");
    }
    let geometry = PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B };
    let profile = qwen25_a16_artifact_row_profile_v7(geometry).expect("the held row at this width");
    let artifact = std::sync::Arc::new(artifact);
    let network = 1u64 << 26;
    let backend = || {
        Qwen25A16Backend::from_registered_profile(artifact.clone(), b"misaka-palw-rc".to_vec(), profile.clone(), (positions, decode))
            .expect("the held row is servable over the real artifact")
            .with_step_ladder_cap(network)
            .with_prompt_ids_form(PalwPromptIdsFormV1::Flat)
    };
    drill("held-real", &artifact, &profile, positions, decode, network, &backend);
}
