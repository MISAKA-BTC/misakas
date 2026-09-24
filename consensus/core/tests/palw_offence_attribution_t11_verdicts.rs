//! **ADR-0152 v2 F2 leaves testnet-11's offence verdicts where they were.**
//!
//! testnet-11 does not arm `Params::palw_offence_attribution`, so on it the V1 `PanelFalseValid`
//! keeps its rule and `PanelFalseValidV2` does not exist. This file pins both halves on
//! `palw_rc_shipped_params()`, folding with the extras the processor resolves there
//! (`offence_attribution_active = false`), past testnet-11's own objective-offence height
//! (`PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA`):
//!
//! * **The V1 kind, row by row.** Every verdict below the kind-3 marker is pinned to the build
//!   before the fence (`5bf72b46`) by running this same file against it (with the kind-3 section,
//!   which names types that build does not have, cut off). The rows are the ones the F2 finding
//!   rests on: the processor's V1 gate ACCEPTS the forced-id object (`job_id := claim_id`, the shape
//!   `review_economic_forged_false_valid` files) and refuses a forged output on a real-shaped claim
//!   (`job_id` = the claim's execution anchor, claim id = `attempt_id_v2`) with `WorkMismatch`; a
//!   receipt signed in the V3 form does not verify as V2; and the V1 fold, handed the real-shaped
//!   object, convicts the seat's lock to the same state root it wrote before the fence existed.
//! * **The V2 kind is dormant** at the processor's gate and in the fold, and the block that carried
//!   it folds to the root it folds to without it — while the same object, folded with the fence
//!   switched on over the same testnet-11 state, is a conviction, so it is the fence and nothing
//!   else that refuses it.
//!
//! The claim is a junk floor attempt (as every `dos_*` fixture makes them) whose execution root is a
//! well-formed flat base0 binding with its first decode token forged — the probe's
//! `garbage_binding` shape, a real `ForgedOutput` fault built from core alone.
//!
//! Run: cargo test -p kaspa-consensus-core --test palw_offence_attribution_t11_verdicts -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA, Params, palw_rc_shipped_params};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, attempt_id_v2, attempt_trace_manifest_root_v1, execution_anchor_v3, execution_commitment_v3,
};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_legs::{PALW_LEGS_OBJECT_VERSION_V1, PalwCheckpointProfileV1};
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
    palw_offence_evidence_digest_v1, palw_panel_contradiction_convicts_execution_v1, palw_verify_objective_offence_v1,
};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2,
    palw_receipt_message_v2, palw_receipt_message_v3,
};
use kaspa_consensus_core::palw_producer_v2::palw_min_trace_retention_daa_v1;
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_layout_id_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateParamsV2,
};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, checkpoint_empty_root_v2, checkpoint_leg_root_v2,
    execution_commitment_root_v2, step_leg_root_v1,
};
use kaspa_consensus_core::palw_step_refute::{PalwBase0DecodeTokensV1, base0_decode_token_select_v1, base0_logits_trace_root_v1};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use libcrux_ml_dsa::ml_dsa_87::{MLDSA87KeyPair, generate_key_pair};

// ---- The rows, as `5bf72b46` printed them --------------------------------------------------------

/// The processor's V1 gate (`palw_verify_objective_offence_v1`) on the forced-id object.
const GATE_FORCED_ID: &str = "Ok(())";
/// …on the forged output of the real-shaped claim (its job id is the execution anchor).
const GATE_REAL_SHAPED: &str = "Err(PanelFalseValidWorkMismatch)";
/// …on the real-shaped object whose receipt was signed in the V3 (segmented) form.
const GATE_V3_SIGNED: &str = "Err(PanelFalseValidReceiptUnverified)";
/// The root binding (`palw_panel_contradiction_convicts_execution_v1`): the forced-id object
/// against the claim's root, and against its own.
const BIND_FORCED_ID_ON_CLAIM_ROOT: &str = "Err(PanelFalseValidWorkMismatch)";
const BIND_FORCED_ID_ON_OWN_ROOT: &str = "Ok(())";
/// …the real-shaped object against the claim's root.
const BIND_REAL_SHAPED: &str = "Ok(())";
/// The licensed claim's state root, the seat's lock and collateral, and the state root and
/// collateral after the V1 fold convicts that seat on the real-shaped object.
///
/// **The three roots re-pinned once for ADR-0152's v22 skeleton**: `PALW_STATE_V2_VERSION` 21 -> 22
/// and the record appends (the claim's `job_identity`/`rcore`, the lock's `attested`/`segments`, the
/// consumed offence's `collected`/`claim_id`, all dormant) move every root; the lock, both
/// collaterals and every verdict string are unchanged, which is the parity this file pins. v21:
/// licensed `5406163a…`, after the conviction `9c9d4a01…`, the next empty block `4c1670c8…`.
/// Within v22, before it shipped (the S-4 review's G freeze), `PalwClaimRcoreV1` gained
/// `g_res_sompi` at its tail — 16 zero bytes per claim on testnet-11, which never writes it — so the
/// three roots moved once more and nothing else did: the lock, both collaterals and every verdict are
/// the same values. Pre-field: licensed `fa02c3ff…`, after the conviction `46e89978…`, the next empty
/// block `a5b94a7a…`.
const ROOT_LICENSED: &str =
    "5a95c61d556352f416bd60181f879bdde8b2651072df8775a325fe8ad4fb1705b69dc7a45709e3540f5d7bf4281a31bd20c8f9bddc64c231fd91ebf2093071a4";
const SEAT_LOCK_SOMPI: u128 = 162_677_341;
const SEAT_COLLATERAL_BEFORE: u64 = 1_000_000_000_000;
const ROOT_AFTER_V1_CONVICTION: &str =
    "6b4662d0e9064ee8a4241b891dc7bf68b57c0df2d3ff9549a2baf26904d3312243a828a5032ac8caa8b527c3e76f116f4b7f40cd83418df2c6494f16072dd2de";
const SEAT_COLLATERAL_AFTER: u64 = 999_837_322_659;
/// The root the next block folds to when it carries nothing.
const ROOT_EMPTY_NEXT: &str =
    "94f497ccc4b92f3aa198b063c469b974bbb2f36f9923f76bb47311cc610a5809f3914a49311cc30b3e6d56443bf49ae2f56f1c70aaea5a27efc07d818586c266";

// ---- The fixture ---------------------------------------------------------------------------------

/// Past testnet-11's objective-offence height, so the lock ledger and the V1 kind are live.
const DAA: u64 = PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA + 100;
const PRE_POW: u64 = 0x7110_0000_5EED;
const SEED: u64 = 0x7110;

fn verify(pk: &[u8], msg: &[u8], sig: &[u8], ctx: &[u8]) -> bool {
    // The processor's `verify_mldsa87_with_context`, inlined as `review_economic_forged_false_valid`
    // does: length checks, then libcrux's PORTABLE verify.
    use libcrux_ml_dsa::ml_dsa_87::{MLDSA87Signature, MLDSA87VerificationKey, portable};
    let (Ok(k), Ok(s)) = (<[u8; 2592]>::try_from(pk), <[u8; 4627]>::try_from(sig)) else { return false };
    portable::verify(&MLDSA87VerificationKey::new(k), msg, ctx, &MLDSA87Signature::new(s)).is_ok()
}

fn sign(kp: &MLDSA87KeyPair, msg: &[u8], ctx: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, msg, ctx, [7u8; 32]).expect("signs").as_ref().to_vec()
}

/// `testnet-11`'s network domain, as the processor derives it.
fn domain(p: &Params) -> Hash64 {
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash))
}

fn rebind(b: &mut PalwStepBindingV2) {
    let ctx_hash = b.job_context.context_hash();
    let profile_hash = b.shape_profile.shape_profile_id();
    let decode_calls = b.job_context.exact_decode_tokens.saturating_sub(1);
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, b.step_leaf_count, &b.step_merkle_root);
    let ckpt_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &b.checkpoint_profile.profile_hash(),
        &b.state_chunk_map_id,
        decode_calls,
        b.checkpoint_count,
        &b.checkpoint_merkle_root,
    );
    b.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &b.full_logits_trace_root, &b.activation_leg_root, &ckpt_root, &step_root);
}

/// A well-formed flat base0 commitment for `job_id` whose first decode token is not the pinned
/// selection of its own logits row — a `ForgedOutput` at position 0.
fn forged_binding(job_id: Hash64) -> (PalwStepBindingV2, PalwBase0DecodeTokensV1) {
    let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("RC profile");
    let mut job = rc_job_context(&profile, 4, 4);
    job.job_id = job_id;
    let vocab = profile.vocab_size as usize;
    let mut x = 0x7110_u64.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut rnd = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % 2001) as i32 - 1000
    };
    let rows: Vec<Vec<i32>> = (0..job.exact_decode_tokens).map(|_| (0..vocab).map(|_| rnd()).collect()).collect();
    let mut toks: Vec<u32> = rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
    toks[0] = (toks[0] + 1) % vocab as u32;
    let ctx_hash = job.context_hash();
    let mut b = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: job.clone(),
        shape_profile: profile.clone(),
        checkpoint_profile: PalwCheckpointProfileV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            checkpoint_interval: 8,
            state_layout_id: integer_kv_state_layout_id_v1(),
        },
        state_chunk_map_id: profile.state_chunk_map_id,
        full_logits_trace_root: base0_logits_trace_root_v1(&job, &rows, &toks),
        activation_leg_root: h(0x7110_0A00),
        step_leaf_count: 1 << 12,
        step_merkle_root: h(0x7110_0B00),
        checkpoint_count: 0,
        checkpoint_merkle_root: checkpoint_empty_root_v2(&ctx_hash),
        committed_execution_root: Hash64::default(),
    };
    rebind(&mut b);
    (b, PalwBase0DecodeTokensV1 { logits_rows: rows, generated_token_ids: toks })
}

/// testnet-11's genesis, a floor attempt whose execution root is [`forged_binding`] of its own
/// execution anchor, bound to a panel of genesis seats and licensed all-Valid past the offence
/// height — so every seat holds a lock on it.
struct Licensed {
    p: Params,
    sp: PalwStateParamsV2,
    state: PalwChainStateV2,
    claim_id: Hash64,
    anchor: Hash64,
    binding: PalwStepBindingV2,
    pin: PalwBase0DecodeTokensV1,
    seats: Vec<PalwBondKeyV2>,
    daa: u64,
}

fn licensed() -> Licensed {
    let p = palw_rc_shipped_params();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let bonds = genesis_bonds(&p);
    let seat_count = usize::from(b.panel.seat_count());
    assert!(bonds.len() > seat_count, "testnet-11 has an executor and a panel's worth of genesis seats");
    let (exec_bond, exec_pk, exec_op) = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => {
                Some((*bond, pubkey.clone(), operator_pubkey.clone()))
            }
            _ => None,
        })
        .expect("a genesis bond");
    let panel: Vec<(PalwBondKeyV2, Hash64)> = bonds[1..=seat_count].iter().map(|(k, o, _)| (*k, *o)).collect();
    let seats: Vec<PalwBondKeyV2> = panel.iter().map(|s| s.0).collect();

    let anchor = execution_anchor_v3(h(NET), h(PRE_POW), floor, &exec_bond.0, 7);
    let (binding, pin) = forged_binding(anchor);
    let (mut env, _, _) = junk_attempt(floor, exec_bond, exec_pk, &exec_op, pwu, SEED, PRE_POW);
    env.attempt.trace_root = binding.full_logits_trace_root;
    env.attempt.execution_root = binding.committed_execution_root;
    env.attempt.trace_manifest_root = attempt_trace_manifest_root_v1(env.attempt.trace_root, PALW_ATTEMPT_V2_TRACE_CHUNKS);
    env.attempt.trace_retention_daa = DAA + palw_min_trace_retention_daa_v1(&sp);
    let key = execution_commitment_v3(&env.attempt, anchor);
    let claim_id = attempt_id_v2(&env.attempt);
    assert_eq!(binding.job_context.job_id, anchor, "the binding answers the claim's execution anchor");
    assert_ne!(anchor, claim_id, "and a real claim's id is never its job id: that is the fixed point F2 removes");

    let g = genesis_state(&p);
    let (s, _, skips) =
        fold(&p, &sp, &g, &ctx(0x7110_0001, DAA, 1, 0), &[], PalwBlockWorkV3::Attempt(&env), key).expect("the attempt folds");
    assert!(skips.is_empty(), "the attempt is admitted: {skips:?}");
    let (s, _, _) = fold(
        &p,
        &sp,
        &s,
        &ctx(0x7110_0002, DAA + 1, 2, 0),
        &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0x71A), seats: seats_of(&panel) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the panel binds");
    let (s, _, _) = fold(
        &p,
        &sp,
        &s,
        &ctx(0x7110_0003, DAA + 2, 3, 0),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: valid_receipts(claim_id, &seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the licence folds");
    assert!(matches!(s.claim(&claim_id).expect("the claim").phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed");
    Licensed { p, sp, state: s, claim_id, anchor, binding, pin, seats, daa: DAA + 3 }
}

/// A V1 payload naming `claim_id`, carrying `contradiction`, with the seat's receipt signed by `kp`
/// under `domain` in the V2 form (or, with `v3`, in the segmented form the V1 gate does not read).
fn v1_payload(
    claim_id: Hash64,
    seat: PalwBondKeyV2,
    kp: &MLDSA87KeyPair,
    domain: Hash64,
    contradiction: PalwPanelContradictionV1,
    v3: bool,
) -> PalwPanelFalseValidEvidenceV1 {
    let signed_daa = 0;
    let signature = if v3 {
        let message = palw_receipt_message_v3(domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa, PalwSegmentMaskV2::single(0));
        sign(kp, message.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT)
    } else {
        let message = palw_receipt_message_v2(domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
        sign(kp, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT)
    };
    PalwPanelFalseValidEvidenceV1 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id,
        network_domain: domain,
        accused_seat: seat.0,
        valid_receipt: PalwSeatReceiptV2 {
            claim: claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: seat,
            signed_daa,
            signature,
        },
        executor_pubkey: Vec::new(),
        contradiction,
    }
}

fn offence(kind: PalwOffenceKindV1, accused: PalwBondKeyV2, evidence: Vec<u8>) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ObjectiveOffence { kind, accused, evidence_id: palw_offence_evidence_digest_v1(&evidence), evidence }
}

/// The processor's V1 gate as it calls it: the seat's registered key, active, the chain's domain,
/// the RULESET's step ladder.
fn gate(p: &Params, kind: PalwOffenceKindV1, seat: PalwBondKeyV2, kp: &MLDSA87KeyPair, evidence: &[u8]) -> String {
    let pk = kp.verification_key.as_ref().to_vec();
    let d = domain(p);
    format!(
        "{:?}",
        palw_verify_objective_offence_v1(
            kind,
            &seat.0,
            &palw_offence_evidence_digest_v1(evidence),
            evidence,
            &pk,
            true,
            d.as_byte_slice(),
            bundle(p).court.max_step_leaf_count(),
            verify,
        )
    )
}

// ---- The V1 kind: rows pinned to the build before the fence --------------------------------------

/// **The processor's V1 gate on testnet-11** accepts the forced-id object and refuses the real-shaped
/// forged output with `WorkMismatch`; the root binding it runs next binds whatever root it is given.
#[test]
fn t11_the_v1_gate_accepts_the_forced_id_object_and_refuses_the_real_shaped_one() {
    let f = licensed();
    let seat_kp = generate_key_pair([0x71; 32]);
    let seat = f.seats[0];
    let claim_root = f.state.claim(&f.claim_id).expect("the claim").execution_root;
    assert_eq!(claim_root, f.binding.committed_execution_root);

    // The forced-id object: the job id IS the claim id — the one binding the V1 gate asks for.
    let (forced, forced_pin) = forged_binding(f.claim_id);
    let forced_contradiction = PalwPanelContradictionV1::ForgedOutput { binding: forced.clone(), pin: forced_pin, position: 0 };
    let forced_bytes =
        borsh::to_vec(&v1_payload(f.claim_id, seat, &seat_kp, domain(&f.p), forced_contradiction.clone(), false)).unwrap();
    let gate_forced = gate(&f.p, PalwOffenceKindV1::PanelFalseValid, seat, &seat_kp, &forced_bytes);
    let bind_forced_on_claim =
        format!("{:?}", palw_panel_contradiction_convicts_execution_v1(&forced_contradiction, claim_root, Hash64::default(), 64));
    let bind_forced_on_own = format!(
        "{:?}",
        palw_panel_contradiction_convicts_execution_v1(&forced_contradiction, forced.committed_execution_root, Hash64::default(), 64)
    );

    // The real-shaped object: the claim's own forged output, whose job id is the anchor.
    let real = PalwPanelContradictionV1::ForgedOutput { binding: f.binding.clone(), pin: f.pin.clone(), position: 0 };
    let real_bytes = borsh::to_vec(&v1_payload(f.claim_id, seat, &seat_kp, domain(&f.p), real.clone(), false)).unwrap();
    let gate_real = gate(&f.p, PalwOffenceKindV1::PanelFalseValid, seat, &seat_kp, &real_bytes);
    let v3_bytes = borsh::to_vec(&v1_payload(f.claim_id, seat, &seat_kp, domain(&f.p), real.clone(), true)).unwrap();
    let gate_v3 = gate(&f.p, PalwOffenceKindV1::PanelFalseValid, seat, &seat_kp, &v3_bytes);
    let bind_real = format!("{:?}", palw_panel_contradiction_convicts_execution_v1(&real, claim_root, Hash64::default(), 64));

    println!("gate forced-id {gate_forced} / real-shaped {gate_real} / V3-signed {gate_v3}");
    println!("bind forced-id on the claim root {bind_forced_on_claim} / on its own {bind_forced_on_own} / real-shaped {bind_real}");
    assert_eq!(gate_forced, GATE_FORCED_ID);
    assert_eq!(gate_real, GATE_REAL_SHAPED);
    assert_eq!(gate_v3, GATE_V3_SIGNED);
    assert_eq!(bind_forced_on_claim, BIND_FORCED_ID_ON_CLAIM_ROOT);
    assert_eq!(bind_forced_on_own, BIND_FORCED_ID_ON_OWN_ROOT);
    assert_eq!(bind_real, BIND_REAL_SHAPED);
    assert_ne!(f.anchor, f.claim_id);
}

/// **The V1 fold on testnet-11, handed the real-shaped object, convicts the seat's lock** — the
/// fold asks the root and not the job id, which is the processor's to ask — and writes the root it
/// wrote before the fence existed. The next block carrying nothing folds to its pinned root too.
#[test]
fn t11_the_v1_fold_is_the_fold_it_was() {
    let f = licensed();
    let seat = f.seats[0];
    let seat_kp = generate_key_pair([0x72; 32]);
    let lock = f.state.slashable_lock(seat, f.claim_id).expect("a licensed seat holds a lock past the offence height").amount;
    let before = f.state.bond(&seat).expect("the seat").collateral;
    let real = PalwPanelContradictionV1::ForgedOutput { binding: f.binding.clone(), pin: f.pin.clone(), position: 0 };
    let evidence = borsh::to_vec(&v1_payload(f.claim_id, seat, &seat_kp, domain(&f.p), real, false)).unwrap();
    let next = ctx(0x7110_0004, f.daa, 4, 0);
    let (convicted, _, _) = fold(
        &f.p,
        &f.sp,
        &f.state,
        &next,
        &[offence(PalwOffenceKindV1::PanelFalseValid, seat, evidence)],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the V1 fold convicts");
    let after = convicted.bond(&seat).expect("the seat").collateral;
    let (empty, _, _) = fold(&f.p, &f.sp, &f.state, &next, &[], PalwBlockWorkV3::None, Hash64::default()).expect("an empty block");
    println!("licensed root {} lock {lock} collateral {before}", f.state.state_root());
    println!("after the V1 conviction root {} collateral {after}", convicted.state_root());
    println!("the next block empty root {}", empty.state_root());
    assert_eq!(f.state.state_root().to_string(), ROOT_LICENSED);
    assert_eq!(lock, SEAT_LOCK_SOMPI);
    assert_eq!(before, SEAT_COLLATERAL_BEFORE);
    assert_eq!(convicted.state_root().to_string(), ROOT_AFTER_V1_CONVICTION);
    assert_eq!(after, SEAT_COLLATERAL_AFTER);
    assert_eq!(empty.state_root().to_string(), ROOT_EMPTY_NEXT);
}

// ---- kind 3: past this line the file needs the fence's build (it names PanelFalseValidV2) ------

use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V2, PalwFalseValidReceiptV1, PalwPanelFalseValidEvidenceV2,
};

/// What the processor writes into the extras at `daa` on `p`: the fence as the params resolve it.
fn processor_extras(p: &Params, daa: u64) -> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
    kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
        offence_attribution_active: p.palw_offence_attribution_active_at(daa),
        ..extras(p, daa)
    }
}

fn fold_with(
    f: &Licensed,
    objects: &[PalwConsensusObjectV2],
    extras: &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1,
) -> Result<PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
    let fl = flags(&f.p, f.daa);
    kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
        &f.state,
        &f.sp,
        None,
        &ctx(0x7110_0004, f.daa, 4, 0),
        objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        fl.unavailable_abstains,
        fl.capability_bound,
        fl.uncertified_weightless,
        fl.da_court,
        extras,
    )
    .map(|(s, _, _)| s)
}

/// **`PanelFalseValidV2` is dormant on testnet-11** — at the processor's gate and in the fold — and
/// the block that carried it folds to the root of the block without it. The same object, folded
/// over the same state with the fence switched on, convicts: the fence is what refuses it.
#[test]
fn t11_the_v2_kind_is_dormant_at_the_gate_and_in_the_fold() {
    let f = licensed();
    for daa in [0, f.daa, u64::MAX] {
        assert!(!f.p.palw_offence_attribution_active_at(daa), "testnet-11 never arms the fence (DAA {daa})");
    }
    let seat = f.seats[0];
    let seat_kp = generate_key_pair([0x73; 32]);
    let message = palw_receipt_message_v2(domain(&f.p), f.claim_id, PalwReceiptVerdictV2::Valid, 0);
    let payload = PalwPanelFalseValidEvidenceV2 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V2,
        claim_id: f.claim_id,
        accused_seat: seat.0,
        receipt: PalwFalseValidReceiptV1::Full(PalwSeatReceiptV2 {
            claim: f.claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: seat,
            signed_daa: 0,
            signature: sign(&seat_kp, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
        }),
        contradiction: PalwPanelContradictionV1::ForgedOutput { binding: f.binding.clone(), pin: f.pin.clone(), position: 0 },
        prompt_ids_opening: None,
        reporter_reveal: Vec::new(),
    };
    let evidence = borsh::to_vec(&payload).unwrap();

    // The processor's gate below the fence: the V1 verifier refuses the kind by name (the gate's own
    // arm returns the same error before it, since the params resolve the fence dormant).
    assert_eq!(gate(&f.p, PalwOffenceKindV1::PanelFalseValidV2, seat, &seat_kp, &evidence), "Err(AttributionDormant)");

    // The fold, with the extras the processor resolves on testnet-11.
    let dormant = processor_extras(&f.p, f.daa);
    assert!(!dormant.offence_attribution_active);
    let object = offence(PalwOffenceKindV1::PanelFalseValidV2, seat, evidence);
    let refused = fold_with(&f, std::slice::from_ref(&object), &dormant);
    assert!(matches!(refused, Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::ObjectiveOffenceDormant)), "{refused:?}");
    // A refused object is dropped with the block standing: the block folds to the root it folds to
    // without it, which is the root pinned above.
    let without = fold_with(&f, &[], &dormant).expect("the block without it");
    assert_eq!(without.state_root().to_string(), ROOT_EMPTY_NEXT, "the carrying block's root is the empty block's");
    let applied_one = kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_v1(
        &f.state,
        &f.sp,
        &ctx(0x7110_0004, f.daa, 4, 0),
        &object,
        flags(&f.p, f.daa).unavailable_abstains,
        flags(&f.p, f.daa).capability_bound,
        flags(&f.p, f.daa).uncertified_weightless,
        flags(&f.p, f.daa).da_court,
        &dormant,
    );
    assert!(applied_one.is_err(), "the per-object rehearsal drops it too: {applied_one:?}");

    // Control: the fence switched on over the same testnet-11 state convicts the same object.
    let armed = kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 { offence_attribution_active: true, ..dormant };
    let convicted = fold_with(&f, std::slice::from_ref(&object), &armed).expect("past the fence the same object convicts");
    assert_ne!(convicted.state_root(), without.state_root());
    assert!(convicted.slashable_lock(seat, f.claim_id).is_none(), "the lock was taken");
    assert!(
        matches!(convicted.claim(&f.claim_id).expect("the claim").phase, PalwClaimPhaseV2::Voided { .. }),
        "and the claim voided before Final"
    );
}

// ---- F1 (ADR-0152 v3.1): the attribution-only tags and kinds leave testnet-11 where it was -------

/// **T18x: a V1 payload carrying tag 9 or 10 is refused on testnet-11 exactly as an undecodable tag
/// was** — at the processor's V1 gate (`PanelFalseValidNeedsContradiction`, before the seat, the
/// receipt or the signature is read) and in the V1 fold ("does not decode"), and the carrying block
/// folds to the empty block's pinned root. The same payload bytes with the tag byte replaced by one
/// no build decodes are refused with the same two answers — so no testnet-11 verdict moved when the
/// tags were appended. Kind 4 (`ExecutorRefuted`) is dormant there as kind 3 is, and kinds 5 and 6
/// are refused by name as records only the fold writes.
#[test]
fn t11_attribution_tags_and_kinds_are_refused_as_they_were() {
    use kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1;
    let f = licensed();
    let seat = f.seats[0];
    let seat_kp = generate_key_pair([0x74; 32]);
    let dormant = processor_extras(&f.p, f.daa);
    let refusal_of = |object: &PalwConsensusObjectV2| format!("{:?}", fold_with(&f, std::slice::from_ref(object), &dormant).err());
    for contradiction in [
        PalwPanelContradictionV1::IdentityMismatch { binding: f.binding.clone() },
        PalwPanelContradictionV1::OutputMismatch { binding: f.binding.clone(), pin: PalwDecodeTokenPinV1::Base0V1(f.pin.clone()) },
    ] {
        let payload = v1_payload(f.claim_id, seat, &seat_kp, domain(&f.p), contradiction.clone(), false);
        let bytes = borsh::to_vec(&payload).unwrap();
        // The same bytes with the contradiction's tag replaced by one nothing decodes.
        let tag_at = bytes.len() - borsh::to_vec(&contradiction).unwrap().len();
        let mut undecodable = bytes.clone();
        undecodable[tag_at] = 0xEE;
        assert!(borsh::from_slice::<PalwPanelFalseValidEvidenceV1>(&undecodable).is_err());
        for evidence in [&bytes, &undecodable] {
            assert_eq!(
                gate(&f.p, PalwOffenceKindV1::PanelFalseValid, seat, &seat_kp, evidence),
                "Err(PanelFalseValidNeedsContradiction)",
                "the V1 gate"
            );
        }
        let appended = refusal_of(&offence(PalwOffenceKindV1::PanelFalseValid, seat, bytes));
        let unknown = refusal_of(&offence(PalwOffenceKindV1::PanelFalseValid, seat, undecodable));
        assert!(appended.contains("PanelFalseValid evidence does not decode"), "{appended}");
        assert!(unknown.contains("PanelFalseValid evidence does not decode"), "{unknown}");
    }
    let without = fold_with(&f, &[], &dormant).expect("the block without them");
    assert_eq!(without.state_root().to_string(), ROOT_EMPTY_NEXT, "the carrying block's root is the empty block's");

    // Kind 4 is dormant on testnet-11; 5 and 6 are never filed.
    let evidence = vec![4u8; 16];
    assert_eq!(gate(&f.p, PalwOffenceKindV1::ExecutorRefuted, f.seats[0], &seat_kp, &evidence), "Err(AttributionDormant)");
    let refused = fold_with(&f, &[offence(PalwOffenceKindV1::ExecutorRefuted, seat, evidence)], &dormant);
    assert!(matches!(refused, Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::ObjectiveOffenceDormant)), "{refused:?}");
    for kind in [PalwOffenceKindV1::DaDefault, PalwOffenceKindV1::CourtConviction] {
        let evidence = vec![kind as u8; 16];
        assert!(gate(&f.p, kind, seat, &seat_kp, &evidence).starts_with("Err(KindNotFileable("), "{kind:?}");
        let refused = refusal_of(&offence(kind, seat, evidence));
        assert!(refused.contains("is a record the fold writes"), "{kind:?}: {refused}");
    }
}
