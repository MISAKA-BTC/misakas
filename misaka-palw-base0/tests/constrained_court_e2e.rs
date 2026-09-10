//! **ADR-0096 §10 B5/B6 over a REAL constrained run, through `adjudicate_court_close_v2` — the
//! fold's own entry — against the build's PINNED Qwen2.5 token table.**
//!
//! consensus-core's own tests (`palw_court_v2::constrained_close_tests`) try both arms over
//! synthetic rows and a synthetic table pinned for their thread. This is the other half the ADR's
//! drill paragraph asks for: the a16 engine masks a real decode of a fixture-sized artifact at the
//! Qwen2.5 vocabulary (151,936 lanes) through the table the tokenizer file builds, the backend
//! commits the claim's roots exactly as a worker would, and the court — which holds no tokenizer —
//! tries the claim through `token_table_pin_for_v1`, the build's row for that tokenizer. What is
//! checked here that the synthetic tests cannot check: that the court's `output_root` recomputation
//! is the engine's, that the court's finish rule is the engine's at the real EOG id (151,643), and
//! that openings of real ids verify against the pinned root.
//!
//! The chain around the claim is real too: a class, two bonds, a free-prompt claim walked to
//! `ReceiptLicensed`, a court opened over the claim's step space and narrowed ON CHAIN by rung moves
//! to the disputed decode call.
//!
//! Needs Qwen2.5's `tokenizer.json` — the file the pin is over — found the way the pin's own test
//! finds it (`MISAKA_FLOOR_TOKENIZER_DENSE`, then the two host paths). A host without it FAILS
//! unless it declares `MISAKA_PALW_WIDTH_NO_TOKENIZER`, which turns the run into a named SKIPPED
//! line: a court test that checked nothing must not read as one that checked.

use std::sync::{Arc, OnceLock};

use kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_bisect::{
    PALW_BISECT_OBJECT_VERSION_V1, PalwBisectDisclosureV1, PalwBisectSpaceV1, PalwBisectVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::{PalwCourtV2Error, PalwCourtVerdictProofV2, adjudicate_court_close_v2, court_session_id_v2};
use kaspa_consensus_core::palw_decode_constraint_v1::{
    PALW_DECODE_CONSTRAINT_VERSION_V1, PalwConstraintActionV1, PalwConstraintCursorV1, PalwConstraintEdgeV1, PalwConstraintFrameV1,
    PalwConstraintNodeV1, PalwDecodeConstraintV1, constraint_admits_any_byte_v1, constraint_admits_lane_v1,
    constraint_cursor_advance_v1, constraint_cursor_start_v1, rendered_segments_hash_v1,
};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION_CONSTRAINED, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwCourtVerdictV2, PalwPanelSeatV2, PalwPwuRuleV2,
    PalwStateParamsV2, apply_palw_transition_v2,
};
use kaspa_consensus_core::palw_step_leg::{
    PalwStepBindingV2, PalwStepOpeningV1, checkpoint_leg_root_v2, execution_commitment_root_v2, step_leg_root_v1, step_merkle_path_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    PALW_LOGITS_TILE_LANES, PalwTiledDecodePinV1, base0_decode_token_select_v1, tiled_logits_row_root_v1, tiled_logits_tile_leaf_v1,
    tiled_logits_trace_root_v1,
};
use kaspa_consensus_core::palw_token_table_v1::{
    PalwTokenTableOpeningV1, PalwTokenTablePinnedV1, token_table_opening_from_leaves_v1, token_table_pin_for_v1, token_table_root_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::engine_a16::derived_a16_store;
use misaka_palw_base0::qwen25_a16_backend::{A16TokenTableV1, Qwen25A16Backend};
use misaka_palw_base0::token_table::{token_table_bytes_v1, token_table_leaves_v1};
use misaka_palw_base0::tokenizer::QwenTokenizer;

const NETWORK: &[u8] = b"misaka-palw-rc";
/// The Qwen2.5 A16 class's vocabulary: the pinned table's width, and the lane space the court opens.
const VOCAB: u32 = 151_936;
const DECODE: u32 = 8;
const CLAIM: u64 = 0xFC;
const ARMED: bool = true;
const DORMANT: bool = false;
const FLAT: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1 =
    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;

// -------------------------------------------------------------------------------------------------
// The tokenizer the pin is over
// -------------------------------------------------------------------------------------------------

const DENSE_TOKENIZER_FIXTURES: &[&str] =
    &["/Users/wata/Downloads/qwen25-tokenizer.json", "/Users/wata/Downloads/misaka-palw-runtime/models/qwen2.5-1.5b/tokenizer.json"];
const NO_CHECKPOINT: &str = "MISAKA_PALW_WIDTH_NO_TOKENIZER";

/// The pin test's lookup rule (`token_table.rs`): `Ok(bytes)`, `Err(skipped line)` only on a host
/// that declared it cannot check, and a panic otherwise.
fn dense_tokenizer_bytes() -> Result<Vec<u8>, String> {
    let path = if let Ok(p) = std::env::var("MISAKA_FLOOR_TOKENIZER_DENSE") {
        let p = std::path::PathBuf::from(p);
        assert!(p.exists(), "MISAKA_FLOOR_TOKENIZER_DENSE={} does not exist", p.display());
        p
    } else if let Some(p) = DENSE_TOKENIZER_FIXTURES.iter().map(std::path::PathBuf::from).find(|p| p.exists()) {
        p
    } else if std::env::var_os(NO_CHECKPOINT).is_some() {
        return Err(format!(
            "SKIPPED constrained_court_e2e: no dense tokenizer on this host and {NO_CHECKPOINT} is set. THE CONSTRAINED \
             COURT WAS NOT TRIED OVER A REAL RUN BY THIS RUN."
        ));
    } else {
        panic!(
            "constrained_court_e2e has no dense tokenizer: MISAKA_FLOOR_TOKENIZER_DENSE is unset and none of [{}] exists. \
             Set it to the dense checkpoint's tokenizer.json, or set {NO_CHECKPOINT} to declare that this host cannot check.",
            DENSE_TOKENIZER_FIXTURES.join(", ")
        )
    };
    Ok(std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

// -------------------------------------------------------------------------------------------------
// The run, once per process
// -------------------------------------------------------------------------------------------------

/// `[`, one to three digits, `]` — and then nothing, so B7's finish is inside the budget.
fn automaton() -> PalwDecodeConstraintV1 {
    let goto = |lo: u8, hi: u8, next: u16| PalwConstraintEdgeV1 { lo, hi, action: PalwConstraintActionV1::Goto(next) };
    let node = |accepting: bool, edges: Vec<PalwConstraintEdgeV1>| PalwConstraintNodeV1 { accepting, edges };
    let c = PalwDecodeConstraintV1 {
        version: PALW_DECODE_CONSTRAINT_VERSION_V1,
        compiler_id: Hash64::from_u64_word(0x0096_0010),
        start_frame: 0,
        frames: vec![PalwConstraintFrameV1 {
            start: 0,
            nodes: vec![
                node(false, vec![goto(b'[', b'[', 1)]),
                node(false, vec![goto(b'0', b'9', 2)]),
                node(false, vec![goto(b'0', b'9', 3), goto(b']', b']', 5)]),
                node(false, vec![goto(b'0', b'9', 4), goto(b']', b']', 5)]),
                node(false, vec![goto(b']', b']', 5)]),
                node(true, Vec::new()),
            ],
        }],
    };
    c.validate().expect("a well-formed automaton");
    c
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

struct Fixture {
    pin: PalwTokenTablePinnedV1,
    table_bytes: Vec<Vec<u8>>,
    leaves: Vec<Hash64>,
    job: PalwFreePromptJobV3,
    binding: PalwStepBindingV2,
    /// The selecting rows the engine retained — row `r` is the one `ids[r]` was chosen from.
    rows: Vec<Vec<i32>>,
    ids: Vec<u32>,
    output_root: Hash64,
}

impl Fixture {
    fn segments(&self, ids: &[u32]) -> Vec<Vec<u8>> {
        ids.iter().map(|id| self.table_bytes[*id as usize].clone()).collect()
    }

    fn opening(&self, id: u32) -> PalwTokenTableOpeningV1 {
        token_table_opening_from_leaves_v1(&self.leaves, id, self.table_bytes[id as usize].clone()).expect("an id of the table opens")
    }

    /// Does the constraint admit `id` after the committed prefix `ids[..p]`?
    fn admitted_at(&self, p: usize, id: u32) -> bool {
        let c = automaton();
        let mut cursor = constraint_cursor_start_v1(&c);
        for committed in &self.ids[..p] {
            cursor = constraint_cursor_advance_v1(
                &c,
                &cursor,
                &self.table_bytes[*committed as usize],
                *committed == self.pin.lowest_eog_id,
            )
            .expect("the honest prefix follows the rule");
        }
        match cursor {
            PalwConstraintCursorV1::Running(state) => {
                constraint_admits_lane_v1(&c, &state, Some(&self.table_bytes[id as usize])).is_some()
            }
            PalwConstraintCursorV1::Finished => false,
        }
    }
}

/// `None` on a host that declared it has no tokenizer.
fn fixture() -> Option<&'static Fixture> {
    static FIXTURE: OnceLock<Option<Fixture>> = OnceLock::new();
    FIXTURE
        .get_or_init(|| {
            let file = match dense_tokenizer_bytes() {
                Ok(bytes) => bytes,
                Err(skipped) => {
                    println!("{skipped}");
                    return None;
                }
            };
            let commitment = Base0ArtifactV1::tokenizer_commitment_of(&file);
            let pin = token_table_pin_for_v1(&commitment).expect("the file is the tokenizer the build pins a table for");
            assert_eq!(pin.vocab_len, VOCAB);
            let tokenizer = QwenTokenizer::from_json(&file).expect("the pinned tokenizer parses");
            let table_bytes: Vec<Vec<u8>> = (0..VOCAB).map(|id| token_table_bytes_v1(&tokenizer, id)).collect();
            let leaves = token_table_leaves_v1(&tokenizer, VOCAB);
            assert_eq!(token_table_root_v1(&leaves), pin.root, "the table the file builds is the pinned one");

            // A fixture-sized A16 class at the Qwen2.5 vocabulary: the same construction as
            // `court_e2e.rs`, 151,936 lanes wide so the pinned table is the class's table.
            let geometry = PalwQwen25GeometryV1 {
                layer_count: 2,
                hidden_dim: 8,
                ffn_dim: 8,
                attn_heads: 2,
                attn_kv_heads: 2,
                attn_head_dim: 4,
                vocab_size: VOCAB,
                n_ctx: 32,
                n_threads: 1,
                rms_eps_q: 1,
                tile_len: 4,
            };
            let profile = qwen25_a16_profile_v2(geometry).expect("the corrected A16 profile projects");
            let shape = Base0ShapeV1 {
                n_layers: geometry.layer_count as usize,
                n_heads: geometry.attn_heads as usize,
                n_kv_heads: geometry.attn_kv_heads as usize,
                d_head: geometry.attn_head_dim as usize,
                d_ff: geometry.ffn_dim as usize,
                vocab: VOCAB as usize,
                max_position: geometry.n_ctx as usize,
                ln_theta_gen_q: LN_THETA_10000_GEN_Q,
                eps_q: 1,
            };
            let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
                .expect("a valid shape")
                .with_a16_params(derived_a16_store(&shape))
                .expect("the derived store is sorted and unique");
            let backend = Qwen25A16Backend::new(Arc::new(artifact), NETWORK.to_vec(), profile.clone(), (4, 3))
                .expect("the fixture's declaration is this engine's program")
                .with_token_table(Arc::new(A16TokenTableV1 { bytes: table_bytes.clone(), lowest_eog: pin.lowest_eog_id }));

            let c = automaton();
            let prompt: Vec<usize> = vec![9_707, 11, 1_879];
            let job = PalwFreePromptJobV3 {
                version: PALW_FP_V3_VERSION_CONSTRAINED,
                network_domain: Hash64::from_u64_word(999),
                class_id: profile.shape_profile_id(),
                executor_bond: bond_key(1).0,
                executor_pubkey: vec![7; 4],
                operator_id: Hash64::from_u64_word(0xE0),
                anchor_block: Hash64::from_u64_word(0xA0),
                anchor_daa: 100,
                job_nonce: [0x11; 32],
                tokenizer_id: commitment,
                prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(
                    &prompt.iter().map(|t| *t as u32).collect::<Vec<_>>(),
                ),
                prompt_tokens: prompt.len() as u32,
                decode_token_limit: DECODE,
                max_context_tokens: 32,
                privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
                prompt_mode: PALW_FP_PROMPT_MODE_USER,
                sampling_seed: [0; 32],
                temperature_q: 0,
                constraint_id: c.id(),
            };
            let run = backend.execute_free_prompt_constrained_streaming(&job, &prompt, &c, &mut |_| {}).expect("the masked run");
            let material =
                misaka_palw_base0::produce::base0_fp_material_decode_v2(&run.outcome.material).expect("our own material decodes");
            assert_eq!(material.generated_token_ids, run.output_token_ids);
            assert_eq!(material.binding.full_logits_trace_root, run.outcome.trace_root);
            assert_eq!(material.binding.committed_execution_root, run.outcome.execution_root);
            Some(Fixture {
                pin,
                table_bytes,
                leaves,
                job,
                binding: material.binding,
                rows: material.logits_rows,
                ids: material.generated_token_ids,
                output_root: run.outcome.output_root,
            })
        })
        .as_ref()
}

// -------------------------------------------------------------------------------------------------
// Pins, claims, courts
// -------------------------------------------------------------------------------------------------

fn tiled_pin(binding: &PalwStepBindingV2, rows: &[Vec<i32>], ids: &[u32], position: u32, beat_lane: u32) -> PalwTiledDecodePinV1 {
    let ctx_hash = binding.job_context.context_hash();
    let row = &rows[position as usize];
    let tiles: Vec<Vec<i32>> = row.chunks(PALW_LOGITS_TILE_LANES).map(<[i32]>::to_vec).collect();
    let tile_leaves: Vec<Hash64> =
        tiles.iter().enumerate().map(|(t, lanes)| tiled_logits_tile_leaf_v1(&ctx_hash, position, t as u32, lanes)).collect();
    let row_roots: Vec<Hash64> = rows
        .iter()
        .enumerate()
        .map(|(r, lanes)| tiled_logits_row_root_v1(&ctx_hash, r as u32, lanes).expect("rows have lanes"))
        .collect();
    let committed = ids[position as usize] as usize;
    let (ct, bt) = (committed / PALW_LOGITS_TILE_LANES, beat_lane as usize / PALW_LOGITS_TILE_LANES);
    let open = |leaves: &[Hash64], index: usize| PalwStepOpeningV1 {
        leaf_index: index as u64,
        leaf_hash: leaves[index],
        siblings: step_merkle_path_v1(leaves, index).expect("inside the leg bounds"),
    };
    PalwTiledDecodePinV1 {
        position,
        generated_token_ids: ids.to_vec(),
        row_root: row_roots[position as usize],
        row_opening: open(&row_roots, position as usize),
        committed_tile_lanes: tiles[ct].clone(),
        committed_opening: open(&tile_leaves, ct),
        beat_tile_lanes: tiles[bt].clone(),
        beat_opening: open(&tile_leaves, bt),
        beat_lane,
    }
}

fn one_disclosure_pin(binding: &PalwStepBindingV2, rows: &[Vec<i32>], ids: &[u32], position: u32) -> PalwTiledDecodePinV1 {
    let mut pin = tiled_pin(binding, rows, ids, position, 0);
    let empty = PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: Vec::new() };
    pin.committed_tile_lanes.clear();
    pin.beat_tile_lanes.clear();
    pin.committed_opening = empty.clone();
    pin.beat_opening = empty;
    pin
}

fn b5(f: &Fixture, binding: &PalwStepBindingV2, pin: PalwTiledDecodePinV1, segments: &[Vec<u8>]) -> PalwCourtVerdictProofV2 {
    let beat_opening = (!pin.beat_tile_lanes.is_empty()).then(|| f.opening(pin.beat_lane));
    PalwCourtVerdictProofV2::ConstrainedDecode {
        binding: Box::new(binding.clone()),
        pin: Box::new(pin),
        job: Box::new(f.job.clone()),
        constraint: automaton().to_bytes(),
        segments: segments.to_vec(),
        beat_opening,
    }
}

fn b6(f: &Fixture, ids: &[u32], segments: &[Vec<u8>], position: u32) -> PalwCourtVerdictProofV2 {
    PalwCourtVerdictProofV2::ConstrainedRendering {
        binding: Box::new(f.binding.clone()),
        job: Box::new(f.job.clone()),
        generated_token_ids: ids.to_vec(),
        segments: segments.to_vec(),
        position,
        opening: f.opening(ids[position as usize]),
    }
}

/// A producer that committed `ids` over the honest rows: the trace root re-keyed with its ids and
/// the execution root rebuilt from the binding's own parts — the lie inside the commitment, exactly
/// as a fraudulent producer would make it.
fn lying_binding(f: &Fixture, ids: &[u32]) -> PalwStepBindingV2 {
    let mut binding = f.binding.clone();
    binding.full_logits_trace_root = tiled_logits_trace_root_v1(&binding.job_context, &f.rows, ids).expect("a tree");
    let ctx_hash = binding.job_context.context_hash();
    let step_root =
        step_leg_root_v1(&ctx_hash, &binding.shape_profile.shape_profile_id(), binding.step_leaf_count, &binding.step_merkle_root);
    let checkpoint_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &binding.checkpoint_profile.profile_hash(),
        &binding.state_chunk_map_id,
        binding.job_context.exact_decode_tokens.saturating_sub(1),
        binding.checkpoint_count,
        &binding.checkpoint_merkle_root,
    );
    binding.committed_execution_root = execution_commitment_root_v2(
        &ctx_hash,
        &binding.full_logits_trace_root,
        &binding.activation_leg_root,
        &checkpoint_root,
        &step_root,
    );
    binding
}

fn output_root(binding: &PalwStepBindingV2, ids: &[u32], segments: &[Vec<u8>]) -> Hash64 {
    kaspa_consensus_core::palw_v2::output_commitment_v2(&binding.job_context.context_hash(), ids, &rendered_segments_hash_v1(segments))
}

fn ctx(daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: kaspa_consensus_core::BlockHash::from_u64_word(daa), daa_score: daa, blue_score: daa, subsidy: 0 }
}

fn court() -> PalwCourtParamsV2 {
    PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("the shipped court")
}

fn apply(state: &PalwChainStateV2, p: &PalwStateParamsV2, daa: u64, objects: &[PalwConsensusObjectV2]) -> PalwChainStateV2 {
    apply_palw_transition_v2(state, p, &ctx(daa), objects, None::<&PalwAttemptEnvelopeV2>).expect("the transition applies").0
}

/// A free-prompt claim over `binding`'s roots and `output_root`, licensed, under a court whose
/// ladder the challenger narrowed ON CHAIN to the first leaf of decode call `call`.
fn narrowed_court(binding: &PalwStepBindingV2, output_root: Hash64, call: u32) -> (PalwChainStateV2, Hash64) {
    let cid = binding.shape_profile.shape_profile_id();
    let p = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, cid, 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_turn_deadline_daa(20)
        .unwrap();
    let bond = |key: u64, pubkey: u8| PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(key),
        pubkey: vec![pubkey; 4],
        operator_pubkey: vec![pubkey; 8],
        collateral: 1_000,
        payout_payload: Hash64::from_u64_word(0x9A10 + key),
        capable_classes: Default::default(),
        signature: Vec::new(),
    };
    let registry = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: cid,
            artifact_root: Hash64::from_u64_word(0xA1),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        bond(1, 7),
        bond(2, 8),
    ];
    let s1 = apply(&PalwChainStateV2::genesis(), &p, 100, &registry);
    let commit = PalwConsensusObjectV2::FreePromptCommitted {
        claim: Hash64::from_u64_word(CLAIM),
        class_id: cid,
        bond: bond_key(1),
        executor_pubkey: vec![7; 4],
        // Priced at three quanta: the transition trusts the acceptance layer's price, and the court
        // never reads it.
        work_leaves: 60,
        prompt_token_ids_hash: binding.job_context.prompt_token_ids_hash,
        decode_tokens_executed: binding.job_context.exact_decode_tokens,
        trace_root: binding.full_logits_trace_root,
        output_root,
        execution_root: binding.committed_execution_root,
        trace_chunk_count: 1,
        trace_retention_daa: 999_999,
    };
    let claim_id = Hash64::from_u64_word(CLAIM);
    let s2 = apply(&s1, &p, 101, &[commit]);
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(2), operator_id: Hash64::from_u64_word(0x22) }];
    let s3 = apply(&s2, &p, 102, &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: Hash64::from_u64_word(77), seats }]);
    let s4 = apply(&s3, &p, 103, &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }]);
    let space = binding.step_leaf_count.next_power_of_two().max(2);
    let sid = court_session_id_v2(
        &claim_id,
        &binding.full_logits_trace_root,
        &bond_key(1),
        &bond_key(2),
        PalwBisectSpaceV1::StepLeaves,
        space,
    );
    let mut state = apply(
        &s4,
        &p,
        104,
        &[PalwConsensusObjectV2::CourtOpened {
            session_id: sid,
            claim: claim_id,
            challenger_bond: bond_key(2),
            space: PalwBisectSpaceV1::StepLeaves,
            space_size: space,
            signature: Vec::new(),
        }],
    );
    let target = kaspa_consensus_core::palw_step::canonical_step_leaf_index(
        &binding.shape_profile,
        &binding.job_context,
        &kaspa_consensus_core::palw_step::PalwStepCoordinateV1 { call_index: call, node_slot: 0, position: 0, tile_index: 0 },
    )
    .expect("the call is inside the run");
    let (mut daa, mut round) = (104u64, 0u32);
    while state.court_session(&sid).unwrap().ladder.terminal_index().is_none() {
        let mid = state.court_session(&sid).unwrap().ladder.expected_midpoint().expect("the responder's turn");
        daa += 1;
        let disclosure = PalwConsensusObjectV2::CourtDisclosed {
            session_id: sid,
            disclosure: PalwBisectDisclosureV1 {
                version: PALW_BISECT_OBJECT_VERSION_V1,
                session_id: sid,
                round,
                midpoint: mid,
                mid_state: Hash64::from_u64_word(0xD000 + u64::from(round)),
            },
            signature: vec![0xAA; 8],
        };
        state = apply(&state, &p, daa, &[disclosure]);
        daa += 1;
        let verdict = PalwConsensusObjectV2::CourtVerdictPosted {
            session_id: sid,
            verdict: PalwBisectVerdictV1 { version: PALW_BISECT_OBJECT_VERSION_V1, session_id: sid, round, agree: target >= mid },
            signature: vec![0xBB; 8],
        };
        state = apply(&state, &p, daa, &[verdict]);
        round += 1;
    }
    assert_eq!(state.court_session(&sid).unwrap().ladder.terminal_index(), Some(target));
    (state, sid)
}

fn close(
    state: &PalwChainStateV2,
    sid: &Hash64,
    proof: &PalwCourtVerdictProofV2,
    armed: bool,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    adjudicate_court_close_v2(state, sid, proof, &court(), kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES, FLAT, armed)
}

// -------------------------------------------------------------------------------------------------
// The tests
// -------------------------------------------------------------------------------------------------

/// **The engine's run is the rule, and its roots are the court's**: every committed id follows
/// B7's rule under the real table — the admitted argmax while some byte continues, the pinned
/// lowest EOG id (151,643) from the finish to the budget — the raw argmax is a forbidden lane at
/// every answer position, and the claim's `output_root` is exactly what the court recomputes from
/// the binding's context, the ids and the table's renderings.
#[test]
fn a_real_constrained_run_is_what_the_court_reads() {
    let Some(f) = fixture() else { return };
    assert_eq!(f.ids.len(), DECODE as usize);
    let rendered: Vec<u8> = f.segments(&f.ids).concat();
    println!("the masked run: ids {:?}, answer {:?}", f.ids, String::from_utf8_lossy(&rendered));
    let c = automaton();
    let mut cursor = constraint_cursor_start_v1(&c);
    let mut finished_at = None;
    for (p, id) in f.ids.iter().enumerate() {
        if let PalwConstraintCursorV1::Running(state) = &cursor
            && constraint_admits_any_byte_v1(&c, state)
        {
            // The committed id is the admitted argmax of its row.
            let admitted: Vec<usize> = (0..VOCAB as usize)
                .filter(|lane| constraint_admits_lane_v1(&c, state, Some(&f.table_bytes[*lane])).is_some())
                .collect();
            let best =
                admitted.iter().copied().max_by_key(|lane| (f.rows[p][*lane], std::cmp::Reverse(*lane))).expect("a lane is admitted");
            assert_eq!(*id as usize, best, "position {p}: the committed id is the admitted argmax");
            let raw = base0_decode_token_select_v1(&f.rows[p]);
            assert!(!admitted.contains(&raw), "position {p}: the raw argmax {raw} is a lane the constraint forbids");
        } else {
            finished_at.get_or_insert(p);
            assert_eq!(*id, f.pin.lowest_eog_id, "B7: the pinned lowest EOG id from the finish on");
        }
        cursor = constraint_cursor_advance_v1(&c, &cursor, &f.table_bytes[*id as usize], *id == f.pin.lowest_eog_id)
            .expect("the run follows the rule");
    }
    assert!(finished_at.is_some(), "the answer ends inside the budget, so B7's finish is tried");
    assert!(rendered.starts_with(b"[") && rendered.ends_with(b"]"), "the answer is the constrained value");
    assert_eq!(output_root(&f.binding, &f.ids, &f.segments(&f.ids)), f.output_root, "the court's output_root is the engine's");
}

/// **B5 over the real run: honest acquitted at every position, by both arms; a forbidden beating
/// lane is not a fault; and the job-less arm, where it still grades (dormant), convicts the honest
/// masked token by the raw argmax — the hole §10's corollary closes, refused by name where armed.**
#[test]
fn b5_acquits_the_real_honest_run_and_the_old_arm_is_refused_where_armed() {
    let Some(f) = fixture() else { return };
    let segments = f.segments(&f.ids);
    for p in 0..DECODE {
        let (state, sid) = narrowed_court(&f.binding, f.output_root, p);
        let bare = b5(f, &f.binding, one_disclosure_pin(&f.binding, &f.rows, &f.ids, p), &segments);
        assert_eq!(close(&state, &sid, &bare, ARMED), Ok(PalwCourtVerdictV2::ChallengerDefeated), "one disclosure at {p}");
        let raw = base0_decode_token_select_v1(&f.rows[p as usize]) as u32;
        let beaten_by_raw = raw != f.ids[p as usize];
        let forbidden = b5(f, &f.binding, tiled_pin(&f.binding, &f.rows, &f.ids, p, raw), &segments);
        assert_eq!(close(&state, &sid, &forbidden, ARMED), Ok(PalwCourtVerdictV2::ChallengerDefeated), "the raw argmax at {p}");
        let old = PalwCourtVerdictProofV2::DecodeTokenTiled {
            binding: f.binding.clone(),
            pin: tiled_pin(&f.binding, &f.rows, &f.ids, p, raw),
        };
        assert_eq!(
            close(&state, &sid, &old, ARMED),
            Err(PalwCourtV2Error::DecodeCloseWithoutTheJob { close: "DecodeTokenTiled" }),
            "armed, the job-less arm is refused by name"
        );
        if beaten_by_raw {
            assert_eq!(
                close(&state, &sid, &old, DORMANT),
                Ok(PalwCourtVerdictV2::ExecutorGuilty),
                "dormant, it convicts an honest token"
            );
        }
    }
}

/// **B5's negative control over the real run**: a producer that commits the RAW argmax at an
/// answer position — ignoring its own constraint — is convicted by the one-disclosure arm with no
/// tile opened; and the unconstrained arm, which reads the same pin, acquits exactly that lie.
#[test]
fn b5_convicts_a_real_producer_that_ignored_its_constraint() {
    let Some(f) = fixture() else { return };
    let p = 1usize;
    let raw = base0_decode_token_select_v1(&f.rows[p]) as u32;
    assert!(!f.admitted_at(p, raw), "the raw argmax is not admitted at {p}");
    let mut lying = f.ids.clone();
    lying[p] = raw;
    let binding = lying_binding(f, &lying);
    let segments = f.segments(&lying);
    let (state, sid) = narrowed_court(&binding, output_root(&binding, &lying, &segments), p as u32);
    let bare = b5(f, &binding, one_disclosure_pin(&binding, &f.rows, &lying, p as u32), &segments);
    assert_eq!(close(&state, &sid, &bare, ARMED), Ok(PalwCourtVerdictV2::ExecutorGuilty));
    let old = PalwCourtVerdictProofV2::DecodeTokenTiled {
        binding: binding.clone(),
        pin: tiled_pin(&binding, &f.rows, &lying, p as u32, f.ids[p]),
    };
    assert_eq!(close(&state, &sid, &old, DORMANT), Ok(PalwCourtVerdictV2::ChallengerDefeated), "the raw argmax beats nothing by v2");
}

/// **B6 over the real run**: the honest renderings are the pinned table's bytes at every position;
/// a claimant that served another rendering at an answer position — its ids honest — is convicted.
#[test]
fn b6_tries_the_real_renderings_against_the_pinned_table() {
    let Some(f) = fixture() else { return };
    let honest = f.segments(&f.ids);
    for q in 0..DECODE {
        let (state, sid) = narrowed_court(&f.binding, f.output_root, q);
        assert_eq!(close(&state, &sid, &b6(f, &f.ids, &honest, q), ARMED), Ok(PalwCourtVerdictV2::ChallengerDefeated), "position {q}");
    }
    let mut lying = honest.clone();
    lying[0] = b"{".to_vec();
    let (state, sid) = narrowed_court(&f.binding, output_root(&f.binding, &f.ids, &lying), 0);
    assert_eq!(close(&state, &sid, &b6(f, &f.ids, &lying, 0), ARMED), Ok(PalwCourtVerdictV2::ExecutorGuilty));
}
