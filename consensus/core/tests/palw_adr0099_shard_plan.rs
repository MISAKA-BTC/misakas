//! **ADR-0099 — a seat holds a shard, not the model.**
//!
//! Section 5's invariants, as tests, over the shipped rows and the K3 stand-in. Numbers are
//! derived from the tree's own profiles and pinned as LIMITATIONS where they are pinned. The
//! generator is `misaka-palw-base0 --bin palw-shard-plan`.

use kaspa_consensus_core::config::params::{devnet_shipped_params, palw_rc_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1;
use kaspa_consensus_core::palw_context_ladder::{palw_a16_context_row_profile_v5, palw_qwen36_context_row_profile_v5};
use kaspa_consensus_core::palw_measured_model_v1::{
    PALW_MEASURED_MODEL_SCHEMA_V1, PalwMeasureInputsV1, PalwMeasuredCheckV1, PalwMeasuredModelV1, PalwModelManifestV1,
    palw_measure_model_v1, palw_measured_model_id_v1, palw_verify_measured_model_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_fit_v1::stand_ins;
use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B, QWEN25_A16_GRAPH_V5_N_CTX};
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_artifact_row_profile_v5};
use kaspa_consensus_core::palw_shard_court_v1::{
    PalwShardCourtAccusationV1, PalwShardCourtError, palw_shard_court_leaf_is_the_shards_v1, palw_shard_court_session_id_v1,
};
use kaspa_consensus_core::palw_shard_plan_v1::{
    PalwArtifactBytesV1, PalwShardPlanError, palw_qwen25_artifact_bytes_v1, palw_qwen36_artifact_bytes_v1, palw_shard_leaf_run_v1,
    palw_shard_plan_for_seat_v1, palw_shard_plan_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepCoordinateV1, canonical_step_leaf_index};
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, trace_scheme_id_v2};
use kaspa_hashes::Hash64;

const GIB: u64 = 1 << 30;

fn job(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    PalwJobContextV2 {
        version: PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"misaka-palw-rc".to_vec(),
        job_id: Hash64::default(),
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: profile.shape_profile_id(),
        trace_scheme_id: trace_scheme_id_v2(),
        cu_ruleset_id: Hash64::default(),
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::default(),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: profile.n_ctx,
    }
}

fn dense() -> (PalwShapeProfileV3, PalwArtifactBytesV1) {
    (
        palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).expect("the dense row builds"),
        palw_qwen25_artifact_bytes_v1(&QWEN25_1_5B),
    )
}

fn hybrid(n_ctx: u32) -> (PalwShapeProfileV3, PalwArtifactBytesV1) {
    (palw_qwen36_context_row_profile_v5(n_ctx).expect("the hybrid row builds"), palw_qwen36_artifact_bytes_v1(&QWEN36_35B_A3B))
}

fn kimi_k3(n_ctx: u32) -> (PalwShapeProfileV3, PalwArtifactBytesV1) {
    let g = PalwQwen36GeometryV1 { n_ctx, ..stand_ins::KIMI_K3_AS_HYBRID_V1 };
    (qwen36_artifact_row_profile_v5(g).expect("the stand-in builds under the ceiling"), palw_qwen36_artifact_bytes_v1(&g))
}

/// **Invariant 1 — the artifact estimate reconciles with the artifacts this tree ships.** The
/// hybrid's measured artifact is about 33 GiB (the fleet's explorer node maps 34 GiB); the dense
/// A16 artifact is the "resident 1.7 GiB" the Studio's gateway backend names. A formula that
/// missed either by more than a fifth would be counting the wrong projections.
#[test]
fn the_artifact_estimate_reconciles_with_the_shipped_artifacts() {
    let hybrid = palw_qwen36_artifact_bytes_v1(&QWEN36_35B_A3B).total();
    assert!((27 * GIB..=40 * GIB).contains(&hybrid), "the hybrid's estimate is {} GiB against a measured ~33", hybrid / GIB);
    let dense = palw_qwen25_artifact_bytes_v1(&QWEN25_1_5B).total();
    assert!((1_400_000_000..=2_000_000_000).contains(&dense), "the dense estimate is {dense} bytes against a measured ~1.7 GiB");
    // The K3 stand-in's formula against its card: the formula counts every layer's 896 experts
    // and lands well above the card's 2.8 T, which is ADR-0099 U-01 — the card's layer split is
    // not public, and the plan is run at both brackets.
    let k3 = palw_qwen36_artifact_bytes_v1(&stand_ins::KIMI_K3_AS_HYBRID_V1).total();
    assert!(k3 > stand_ins::KIMI_K3_TOTAL_PARAMETERS, "{k3} vs the card's {}", stand_ins::KIMI_K3_TOTAL_PARAMETERS);
    let scaled = palw_qwen36_artifact_bytes_v1(&stand_ins::KIMI_K3_AS_HYBRID_V1).scaled_to_total(stand_ins::KIMI_K3_TOTAL_PARAMETERS);
    assert!(scaled.total().abs_diff(stand_ins::KIMI_K3_TOTAL_PARAMETERS) < 1_000, "scaling meets the card's total");
}

/// **Invariant 2 — a plan partitions the layers contiguously, holds `pre` and `post` exactly once,
/// and its shards' slots tile the profile's slots.** For every shard count a family admits.
#[test]
fn a_plan_partitions_the_layers_and_the_slots_exactly_once() {
    for (name, (profile, artifact)) in [("dense", dense()), ("hybrid", hybrid(8)), ("k3", kimi_k3(512))] {
        for shards in [1u32, 2, 3, 5, 8, 13, u32::from(profile.layer_count)] {
            let plan = palw_shard_plan_v1(&profile, &artifact, shards).unwrap_or_else(|e| panic!("{name} at {shards}: {e}"));
            assert_eq!(plan.shards.len(), shards as usize);
            assert_eq!(plan.shards[0].first_layer, 0);
            assert!(plan.shards[0].holds_pre && plan.shards[shards as usize - 1].holds_post);
            assert_eq!(plan.shards.iter().filter(|s| s.holds_pre).count(), 1);
            assert_eq!(plan.shards.iter().filter(|s| s.holds_post).count(), 1);
            for w in plan.shards.windows(2) {
                assert_eq!(w[0].first_layer + w[0].layer_count, w[1].first_layer, "{name}: contiguous");
                assert_eq!(w[0].first_slot + w[0].slot_count, w[1].first_slot, "{name}: slots contiguous");
            }
            let last = &plan.shards[shards as usize - 1];
            assert_eq!(last.first_layer + last.layer_count, profile.layer_count, "{name}: every layer");
            assert_eq!(last.first_slot + last.slot_count, profile.global_node_count(), "{name}: every slot");
            assert!(plan.shards.iter().all(|s| s.layer_count >= 1));
            assert_eq!(
                plan.shards.iter().map(|s| s.artifact_bytes).sum::<u64>(),
                artifact.total(),
                "{name}: the artifact is split, not lost"
            );
            assert_eq!(plan.shards.iter().map(|s| u32::from(s.layer_count)).sum::<u32>(), u32::from(profile.layer_count));
            assert_eq!(plan.widest_seat_bytes, plan.shards.iter().map(|s| s.seat_bytes()).max().unwrap());
            assert_eq!(plan.boundary_row_bytes, u64::from(profile.hidden_dim) * 4);
        }
        assert!(matches!(
            palw_shard_plan_v1(&profile, &artifact, u32::from(profile.layer_count) + 1),
            Err(PalwShardPlanError::TooManyShards { .. })
        ));
        assert!(matches!(palw_shard_plan_v1(&profile, &artifact, 0), Err(PalwShardPlanError::ZeroShards)));
    }
}

/// **Invariant 3 — the plan balances: the widest seat never grows with the shard count, and at
/// `k` shards it is within one layer of the perfect split.**
#[test]
fn the_plan_balances_and_more_shards_never_widen_a_seat() {
    for (name, (profile, artifact)) in [("dense", dense()), ("hybrid", hybrid(8)), ("k3", kimi_k3(4_096))] {
        let one = palw_shard_plan_v1(&profile, &artifact, 1).unwrap();
        let heaviest_layer = one.shards[0].seat_bytes() / u64::from(profile.layer_count) * 3; // a generous single-layer bound
        let mut previous = one.widest_seat_bytes;
        for shards in 2..=16u32.min(u32::from(profile.layer_count)) {
            let plan = palw_shard_plan_v1(&profile, &artifact, shards).unwrap();
            assert!(plan.widest_seat_bytes <= previous, "{name}: {shards} shards widened a seat");
            let perfect = one.widest_seat_bytes.div_ceil(u64::from(shards));
            assert!(
                plan.widest_seat_bytes <= perfect + heaviest_layer,
                "{name} at {shards}: {} vs perfect {perfect}",
                plan.widest_seat_bytes
            );
            previous = plan.widest_seat_bytes;
        }
    }
}

/// **Invariant 4 — a shard's leaves are one contiguous run at every step, and the runs of all
/// shards tile the step's leaves exactly.** Checked against the enumeration's own inverse
/// (`canonical_step_leaf_index`) at the next step's first slot.
#[test]
fn shard_leaf_runs_tile_every_step_of_a_job() {
    for (name, (profile, artifact), prefill, decode) in [("dense", dense(), 6u32, 3u32), ("hybrid", hybrid(8), 4, 3)] {
        let ctx = job(&profile, prefill, decode);
        for shards in [1u32, 2, 4] {
            let plan = palw_shard_plan_v1(&profile, &artifact, shards).unwrap();
            let steps: Vec<(u32, u32)> = (0..prefill).map(|p| (0, p)).chain((1..decode).map(|c| (c, 0))).collect();
            for (i, (call, position)) in steps.iter().copied().enumerate() {
                let first_of_step = canonical_step_leaf_index(
                    &profile,
                    &ctx,
                    &PalwStepCoordinateV1 { call_index: call, position, node_slot: 0, tile_index: 0 },
                )
                .expect("slot 0 exists at every step");
                let mut cursor = first_of_step;
                for shard in &plan.shards {
                    let (first, count) = palw_shard_leaf_run_v1(&profile, &ctx, shard, call, position).expect("a step the job has");
                    assert_eq!(first, cursor, "{name} {shards} shards: shard {} at step {i} starts where the last ended", shard.index);
                    cursor += count;
                }
                let next_first = steps.get(i + 1).map(|(c, p)| {
                    canonical_step_leaf_index(
                        &profile,
                        &ctx,
                        &PalwStepCoordinateV1 { call_index: *c, position: *p, node_slot: 0, tile_index: 0 },
                    )
                    .expect("the next step's slot 0")
                });
                if let Some(next) = next_first {
                    assert_eq!(cursor, next, "{name} {shards} shards: the runs tile step {i} exactly");
                }
            }
            assert_eq!(palw_shard_leaf_run_v1(&profile, &ctx, &plan.shards[0], decode, 0), None, "past the last decode call");
            assert_eq!(palw_shard_leaf_run_v1(&profile, &ctx, &plan.shards[0], 0, prefill), None, "past the prefill");
        }
    }
}

/// **Invariant 5 — the seat-budget search, and the limitation it finds for the stand-in.** At
/// 131,072 positions, the Kimi K3 stand-in under its card's total needs the pinned number of
/// shards before a 128 GiB seat can hold one — and more under the formula's total. Pinned so the
/// day the estimate, the map or the ceiling moves, this says so.
#[test]
fn the_seat_budget_search_finds_the_fewest_shards_and_pins_the_stand_ins_number() {
    let (profile, formula) = kimi_k3(131_072);
    let card = formula.scaled_to_total(stand_ins::KIMI_K3_TOTAL_PARAMETERS);
    let plan = palw_shard_plan_for_seat_v1(&profile, &card, 128 * GIB, 92).expect("some plan fits a 128 GiB seat");
    assert!(plan.widest_seat_bytes <= 128 * GIB);
    if plan.shard_count > 1 {
        let one_fewer = palw_shard_plan_v1(&profile, &card, plan.shard_count - 1).unwrap();
        assert!(one_fewer.widest_seat_bytes > 128 * GIB, "the fewest: one fewer shard does not fit");
    }
    assert_eq!(plan.shard_count, 23, "the stand-in at the card's total, 131,072 positions, a 128 GiB seat");
    // And the transfer forms of its widest shard, per job at that context.
    let widest = plan.shards.iter().max_by_key(|s| s.seat_bytes()).unwrap();
    let (recompute, resume) = widest.resume_transfer_bytes(131_072, plan.boundary_row_bytes);
    assert_eq!(recompute, 131_072 * 7_168 * 4, "the previous shard's rows for every position");
    assert_eq!(resume, widest.kv_cache_bytes + widest.recurrent_state_bytes);
    assert_eq!(plan.shards[0].resume_transfer_bytes(131_072, plan.boundary_row_bytes).0, 0, "shard 0 resumes from the prompt");
    // ADR-0099 §1.2's sentence about that plan: four layers a shard, a resume opening under a GiB
    // (one attention layer's cache and three delta-rule states), and the panel's recompute form
    // is every boundary but shard 0's.
    assert!(plan.shards.iter().all(|s| s.layer_count == 4), "92 layers over 23 shards is four each");
    assert!(resume > 3 * GIB / 4 && resume < GIB, "a four-layer K3 shard resumes from under a GiB: {resume}");
    assert_eq!(plan.boundary_bytes_per_job(131_072), 22 * 131_072 * 7_168 * 4, "(shards − 1) × positions × hidden × 4");
    let under_formula = palw_shard_plan_for_seat_v1(&profile, &formula, 128 * GIB, 92).expect("fits by 92");
    assert!(under_formula.shard_count >= plan.shard_count, "the formula's larger artifact needs no fewer shards");
    assert!(
        matches!(palw_shard_plan_for_seat_v1(&profile, &card, 8 * GIB, 92), Err(PalwShardPlanError::BudgetTooSmall { .. })),
        "no 92-way plan fits an 8 GiB seat at 131,072 positions"
    );
    // The shipped rows fit one seat of the fleet's smallest host.
    let (dense_profile, dense_artifact) = dense();
    assert_eq!(palw_shard_plan_for_seat_v1(&dense_profile, &dense_artifact, 24 * GIB, 28).unwrap().shard_count, 1);
    let (hybrid_profile, hybrid_artifact) = hybrid(8);
    assert_eq!(palw_shard_plan_for_seat_v1(&hybrid_profile, &hybrid_artifact, 64 * GIB, 40).unwrap().shard_count, 1);
}

/// **Invariant 7 — the accusation's session id binds every field but the signature, and the
/// shape rules refuse by name.**
#[test]
fn an_accusation_binds_its_fields_and_its_shape_is_checked_by_name() {
    use kaspa_consensus_core::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1, PalwStepTileLeafV1};
    use kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    let bond = |v: u8| PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([v; 64]), 0));
    let (profile, _) = dense();
    let ctx = job(&profile, 4, 2);
    let refutation = |leaf: u64| PalwExecutionStepRefutationV1 {
        binding: PalwStepBindingV2 {
            version: 2,
            job_context: ctx.clone(),
            shape_profile: profile.clone(),
            checkpoint_profile: kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1 {
                version: 1,
                checkpoint_interval: 1,
                state_layout_id: Hash64::default(),
            },
            state_chunk_map_id: Hash64::default(),
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            step_leaf_count: 1_000,
            step_merkle_root: Hash64::default(),
            checkpoint_count: 0,
            checkpoint_merkle_root: Hash64::default(),
            committed_execution_root: Hash64::default(),
        },
        output_opening: PalwStepOpeningV1 { leaf_index: leaf, leaf_hash: Hash64::default(), siblings: vec![] },
        output_preimage: PalwStepTileLeafV1 {
            version: 1,
            coord: PalwStepCoordinateV1 { call_index: 0, position: 0, node_slot: 0, tile_index: 0 },
            value_count: 0,
            values_le: vec![],
        },
        inputs: vec![],
        prompt_token_ids: vec![],
        decode_tokens: None,
        kv_checkpoint: None,
    };
    let base = PalwShardCourtAccusationV1 {
        version: 1,
        claim: Hash64::from_u64_word(1),
        // The binding above commits to the default root, and the shape rule wants them equal.
        execution_root: Hash64::default(),
        trace_root: Hash64::from_u64_word(3),
        executor_bond: bond(1),
        accuser_bond: bond(2),
        leaf_index: 77,
        refutation: refutation(77),
        artifact_openings: vec![],
        prompt_ids_opening: None,
        signature: vec![1, 2, 3],
    };
    assert_eq!(base.validate_shape(1 << 26), Ok(()));
    let domain = b"misaka-palw/test-network";
    let id = palw_shard_court_session_id_v1(domain, &base);
    let mut signed = base.clone();
    signed.signature = vec![9];
    assert_eq!(palw_shard_court_session_id_v1(domain, &signed), id, "the signature is over the id, not in it");
    assert_ne!(palw_shard_court_session_id_v1(b"misaka-palw/another-network", &base), id, "the network domain is inside it");
    let variants: Vec<(&str, PalwShardCourtAccusationV1)> = vec![
        ("claim", PalwShardCourtAccusationV1 { claim: Hash64::from_u64_word(11), ..base.clone() }),
        ("execution_root", PalwShardCourtAccusationV1 { execution_root: Hash64::from_u64_word(12), ..base.clone() }),
        ("trace_root", PalwShardCourtAccusationV1 { trace_root: Hash64::from_u64_word(13), ..base.clone() }),
        ("executor", PalwShardCourtAccusationV1 { executor_bond: bond(3), ..base.clone() }),
        ("accuser", PalwShardCourtAccusationV1 { accuser_bond: bond(4), ..base.clone() }),
        ("leaf", PalwShardCourtAccusationV1 { leaf_index: 78, refutation: refutation(78), ..base.clone() }),
    ];
    for (name, v) in variants {
        assert_ne!(palw_shard_court_session_id_v1(domain, &v), id, "{name} is inside the session id");
    }
    let bad = |a: PalwShardCourtAccusationV1| a.validate_shape(1 << 26).unwrap_err();
    assert!(matches!(bad(PalwShardCourtAccusationV1 { version: 2, ..base.clone() }), PalwShardCourtError::Version { .. }));
    assert!(matches!(
        bad(PalwShardCourtAccusationV1 { execution_root: Hash64::from_u64_word(2), ..base.clone() }),
        PalwShardCourtError::BindingRootMismatch { .. }
    ));
    assert!(matches!(
        bad(PalwShardCourtAccusationV1 { leaf_index: 1 << 26, refutation: refutation(1 << 26), ..base.clone() }),
        PalwShardCourtError::LeafPastTheLadder { .. }
    ));
    assert!(matches!(
        bad(PalwShardCourtAccusationV1 { accuser_bond: bond(1), ..base.clone() }),
        PalwShardCourtError::AccuserIsTheAccused
    ));
    assert!(matches!(
        bad(PalwShardCourtAccusationV1 { refutation: refutation(5), ..base.clone() }),
        PalwShardCourtError::RefutationNamesAnotherLeaf { named: 77, refuted: 5 }
    ));

    // The FILER's rule: a seat holding shard 1 names only shard 1's leaves. The chain asks nothing
    // of the kind, so this lives beside the object and not in its shape.
    let (_, artifact) = dense();
    let plan = palw_shard_plan_v1(&profile, &artifact, 4).unwrap();
    let (first_of_shard_1, _) = palw_shard_leaf_run_v1(&profile, &ctx, &plan.shards[1], 0, 0).expect("shard 1 has a run");
    assert_eq!(palw_shard_court_leaf_is_the_shards_v1(&profile, &ctx, &plan.shards[1], first_of_shard_1), Ok(()));
    assert!(matches!(
        palw_shard_court_leaf_is_the_shards_v1(&profile, &ctx, &plan.shards[0], first_of_shard_1),
        Err(PalwShardCourtError::LeafOutsideTheShard { shard: 0, .. })
    ));
}

/// **Invariant 8 — the fence is dormant on both shipped presets.** Its refusal at assembly and
/// its fingerprint visibility are `config::params`'s own tests.
#[test]
fn the_shard_court_fence_is_dormant_on_both_shipped_presets() {
    for (name, params) in [("testnet-11 (RC)", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        assert!(params.palw_shard_court.is_none(), "{name}");
        assert!(!params.palw_shard_court_active_at(u64::MAX), "{name}");
    }
}

/// **Invariant 6 — a manifest of a shipped class names the shipped class.** The manifest → geometry
/// → graph-v5 projection reproduces the registered rows' own class ids, so "a manifest is enough
/// to name the class" is a measured statement and not a hope.
#[test]
fn a_manifest_of_a_shipped_class_names_the_shipped_class() {
    let dense = PalwModelManifestV1::from_dense("dense", &QWEN25_1_5B);
    assert_eq!(dense.profile(QWEN25_A16_GRAPH_V5_N_CTX).unwrap().shape_profile_id(), dense_profile().shape_profile_id());
    let hybrid = PalwModelManifestV1::from_hybrid("hybrid", &QWEN36_35B_A3B, None);
    assert_eq!(
        hybrid.profile(QWEN36_35B_A3B.n_ctx).unwrap().shape_profile_id(),
        hybrid_profile(QWEN36_35B_A3B.n_ctx).shape_profile_id()
    );
    // And the manifest survives JSON, which is how an adder hands it in.
    let text = serde_json::to_string(&hybrid).unwrap();
    let back: PalwModelManifestV1 = serde_json::from_str(&text).unwrap();
    assert_eq!(back, hybrid);
    let minimal: PalwModelManifestV1 = serde_json::from_str(
        r#"{"name":"d","family":"dense-a16","layer_count":28,"hidden_dim":1536,"vocab_size":151936,"attn_heads":12,"attn_kv_heads":2,"attn_head_dim":128,"ffn_dim":8960}"#,
    )
    .unwrap();
    assert_eq!(
        minimal.profile(512).unwrap().shape_profile_id(),
        dense_profile().shape_profile_id(),
        "the hybrid fields default to zero for a dense manifest"
    );
}

/// **ADR-0102 — a hybrid manifest names its graph, and the graph-v6 variant moves no earlier
/// document.** `hybrid-qwen36-token-lift` projects graph-v6 (a different class over the same
/// geometry, reaching the per-token lift); the variant is appended LAST, so a `hybrid-qwen36`
/// document's Borsh bytes — and therefore its id and its signature — are what they were.
#[test]
fn a_token_lift_manifest_names_graph_v6_and_moves_no_earlier_document() {
    use kaspa_consensus_core::palw_measured_model_v1::PalwModelFamilyV1;
    let v5 = PalwModelManifestV1::from_hybrid("hybrid", &QWEN36_35B_A3B, None);
    let v6 = PalwModelManifestV1::from_hybrid_token_lift("hybrid", &QWEN36_35B_A3B, None);
    assert!(v5.is_hybrid() && v6.is_hybrid());
    assert_eq!(v5.hybrid_geometry(512), v6.hybrid_geometry(512), "one geometry");
    assert_eq!(v5.artifact_bytes(), v6.artifact_bytes(), "one artifact");
    let (p5, p6) = (v5.profile(512).unwrap(), v6.profile(512).unwrap());
    assert_ne!(p5.shape_profile_id(), p6.shape_profile_id(), "two graphs, two classes");
    assert_eq!(
        p6.shape_profile_id(),
        kaspa_consensus_core::palw_qwen36_profile::qwen36_artifact_row_profile_v6(v6.hybrid_geometry(512).unwrap())
            .unwrap()
            .shape_profile_id()
    );
    let lift = kaspa_consensus_core::palw_step::kernel_semantics_id_v1(kaspa_consensus_core::palw_step_refute::KDESC_A16_REQUANTIZE_BY_TOKEN);
    assert!(kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&p6).contains(&lift));
    assert!(!kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&p5).contains(&lift));
    // JSON names it; Borsh appends it.
    assert!(serde_json::to_string(&v6).unwrap().contains(r#""family":"hybrid-qwen36-token-lift""#));
    assert_eq!(serde_json::from_str::<PalwModelManifestV1>(&serde_json::to_string(&v6).unwrap()).unwrap(), v6);
    assert_eq!(borsh::to_vec(&PalwModelFamilyV1::DenseA16).unwrap(), vec![0]);
    assert_eq!(borsh::to_vec(&PalwModelFamilyV1::HybridQwen36).unwrap(), vec![1]);
    assert_eq!(borsh::to_vec(&PalwModelFamilyV1::HybridQwen36TokenLift).unwrap(), vec![2]);
}

fn dense_profile() -> PalwShapeProfileV3 {
    palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).unwrap()
}

fn hybrid_profile(n_ctx: u32) -> PalwShapeProfileV3 {
    palw_qwen36_context_row_profile_v5(n_ctx).unwrap()
}

/// **Invariant 9 — a Measured Model Artifact is recomputed field by field; a tampered one is
/// refused by the field; the self-reported half is carried and never decides.**
#[test]
fn a_measured_model_is_recomputed_field_by_field_and_a_tampered_one_is_named() {
    let params = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("the RC ships a bundle") };
    let fingerprint = format!("{}", params.consensus_params_id());
    let court_for = |profile: &PalwShapeProfileV3| {
        let shape = palw_admission_shape_at_v1(&params, bundle, profile, u64::MAX - 1).expect("an admission shape");
        (shape.court, params.palw_prompt_ids_form_at(u64::MAX - 1))
    };
    let inputs = PalwMeasureInputsV1 {
        ruleset: "testnet-11 (RC)",
        ruleset_fingerprint_hex: &fingerprint,
        bundle,
        contexts: &[512, 2_048, 1_048_576],
        seat_budgets: &[24 * GIB, 512 * GIB],
        max_shards: 64,
        held: None,
    };
    let manifest = PalwModelManifestV1::from_dense("Qwen2.5-1.5B A16 graph-v5", &QWEN25_1_5B);
    let doc = palw_measure_model_v1(&manifest, inputs, &court_for, Some(34), "an operator's laptop");
    assert_eq!(doc.schema, PALW_MEASURED_MODEL_SCHEMA_V1);
    assert_eq!(doc.deterministic.rows.len(), 3);
    assert!(doc.deterministic.rows[0].fit_admitted, "the dense row at 512 fits the RC");
    assert!(
        !doc.deterministic.rows[1].fit_admitted && !doc.deterministic.rows[1].refusing_walls.is_empty(),
        "2,048 is refused by name"
    );
    assert_eq!(doc.deterministic.rows[2].shape_profile_id_hex, None, "1M has no profile");
    assert_eq!(doc.deterministic.rows[2].refusing_walls, vec!["geometry ceiling".to_string()]);
    assert_eq!(doc.deterministic.rows[0].plans[0].shard_count, Some(1), "the dense row fits one 24 GiB seat");
    assert_eq!(doc.self_reported.positions_within_window_receipt, Some(bundle.state.window_receipt() * (120_000 / 34)));

    // The document verifies against its own ruleset, and the self-reported half is listed as such.
    let verdict = palw_verify_measured_model_v1(&doc, inputs, &court_for);
    assert!(verdict.deterministic_ok(), "{:?}", verdict.mismatches());
    assert!(
        verdict
            .checks
            .iter()
            .any(|c| matches!(c, PalwMeasuredCheckV1::SelfReported { field, .. } if field == "self_reported.replay_ms_per_position"))
    );
    assert!(verdict.checks.iter().filter(|c| matches!(c, PalwMeasuredCheckV1::Recomputed { .. })).count() > 20);

    // JSON round trip: what the adder writes is what the node reads.
    let text = serde_json::to_string_pretty(&doc).unwrap();
    let back: PalwMeasuredModelV1 = serde_json::from_str(&text).unwrap();
    assert_eq!(back, doc);
    let id = palw_measured_model_id_v1(&doc);
    assert_eq!(palw_measured_model_id_v1(&back), id);
    let mut signed = doc.clone();
    signed.signature_hex = "ab".repeat(64);
    assert_eq!(palw_measured_model_id_v1(&signed), id, "the signature is over the id, not in it");
    let mut renamed = doc.clone();
    renamed.manifest.name = "another".into();
    assert_ne!(palw_measured_model_id_v1(&renamed), id);

    // Tampering: each deterministic field is refused BY NAME.
    let mut fat = doc.clone();
    fat.deterministic.artifact_bytes += 1;
    assert_eq!(palw_verify_measured_model_v1(&fat, inputs, &court_for).mismatches(), vec!["artifact_bytes".to_string()]);
    let mut admitted = doc.clone();
    admitted.deterministic.rows[1].fit_admitted = true;
    admitted.deterministic.rows[1].refusing_walls.clear();
    assert_eq!(
        palw_verify_measured_model_v1(&admitted, inputs, &court_for).mismatches(),
        vec!["rows[2048].fit_admitted".to_string(), "rows[2048].refusing_walls".to_string()]
    );
    let mut fewer = doc.clone();
    fewer.deterministic.rows[0].plans[0].shard_count = Some(2);
    assert_eq!(palw_verify_measured_model_v1(&fewer, inputs, &court_for).mismatches(), vec!["rows[512].plans".to_string()]);
    let mut elsewhere = doc.clone();
    elsewhere.ruleset_fingerprint_hex = "00".repeat(32);
    assert_eq!(
        palw_verify_measured_model_v1(&elsewhere, inputs, &court_for).mismatches(),
        vec!["ruleset_fingerprint_hex".to_string()]
    );
    // A different self-report is not a mismatch — it is not recomputable — but its derived window
    // figure must match the self-report it came from.
    let mut faster = doc.clone();
    faster.self_reported.replay_ms_per_position = Some(1);
    let v = palw_verify_measured_model_v1(&faster, inputs, &court_for);
    assert_eq!(
        v.mismatches(),
        vec!["self_reported.positions_within_window_receipt".to_string()],
        "the derived figure no longer matches the report"
    );
    faster.self_reported.positions_within_window_receipt = Some(bundle.state.window_receipt() * 120_000);
    assert!(
        palw_verify_measured_model_v1(&faster, inputs, &court_for).deterministic_ok(),
        "a consistent self-report is carried, not judged"
    );
}
