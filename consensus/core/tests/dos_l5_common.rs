//! **L5 DoS lane — shared fixture.** Included by every `dos_l5_*` test through `#[path]`; as its own
//! test target it holds no test (cargo auto-discovers every `tests/*.rs`).
//!
//! The fixture is testnet-12 itself: `Params::from(testnet-12)`, its bundle's own
//! `PalwStateParamsV2`, its own genesis object list folded exactly as `process_genesis` folds it
//! (processor.rs:12674), and a `PalwTransitionExtrasV1` resolved from the same `Params` fences the
//! processor's `palw_transition_extras_for` (processor.rs:8257) reads. Four extras fields are
//! processor-only (they need the header store or the execution lane's span): `round_lane`,
//! `model_registry`, `economic_payout`, `work_target`. They are left `None` here, which is stated in
//! every test that could be affected.
#![allow(dead_code)]

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    attempt_trace_manifest_root_v1, challenge_v2, execution_anchor_v3, execution_commitment_v3,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwEconomicSafetyFoldV1,
    PalwPanelSeatV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error,
    PalwTransitionExtrasV1, apply_palw_transition_v7, palw_operator_id_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

pub const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
pub const NET: u64 = 0xD05_0012;

pub fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

pub fn bundle(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("testnet-12 is ConsensusV2"),
    }
}

pub fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

pub fn msk(sompi: u128) -> f64 {
    sompi as f64 / 1e8
}

/// The four fold flags `process_genesis` and every chain block pass, resolved from `Params`.
pub struct Flags {
    pub unavailable_abstains: bool,
    pub capability_bound: bool,
    pub uncertified_weightless: bool,
    pub da_court: bool,
}

pub fn flags(p: &Params, daa: u64) -> Flags {
    Flags {
        unavailable_abstains: p.palw_unavailable_abstains.is_some_and(|f| f.is_active(daa)),
        capability_bound: p.palw_capability_bound_at(daa),
        uncertified_weightless: p.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa)),
        da_court: p.palw_da_court.is_some_and(|f| f.is_active(daa)),
    }
}

/// `palw_transition_extras_for`, minus the four processor-only fields (see the module doc).
pub fn extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        model_lines_active: p.palw_model_lines_active_at(daa),
        model_benefits_active: p.palw_model_benefits_active_at(daa),
        evm_market_active: p.palw_model_evm_active_at(daa),
        model_leg_v2_active: p.palw_model_leg_v2_active_at(daa),
        model_seed_v2_active: p.palw_model_seed_v2_active_at(daa),
        court_responder_coverage_active: p.palw_court_responder_coverage_active_at(daa),
        share_growth_final_active: false,
        epoch_budget_release_active: false,
        panel_economy_active: p.palw_panel_economy_active_at(daa),
        work_priced_reward_active: p.palw_work_priced_reward_active_at(daa),
        panel_reward_multiple_permille: p.palw_panel_reward_multiple_permille_at(daa),
        economic_safety: p.palw_economic_safety.is_some_and(|f| f.is_active(daa)).then(|| PalwEconomicSafetyFoldV1 {
            target_time_per_block_ms: p.target_time_per_block(),
            permit_value_sompi: kaspa_consensus_core::palw_economic_safety_v1::PALW_T12_PERMIT_FEE_CEILING_SOMPI,
        }),
        escrow_carve: kaspa_consensus_core::config::params::palw_overlay_escrow_carve_at_v1(p.palw_overlay_carve, daa, daa),
        artifact_root_ownership_active: p.palw_artifact_root_ownership_at(daa),
        operator_id_unique_active: p.palw_operator_id_unique_at(daa),
        canonical_work_daa: p.palw_canonical_work_daa(),
        admission_independence_daa: p.palw_admission_independence_daa(),
        single_lottery_active: p.palw_single_lottery_at(daa),
        verification_v2_active: p.palw_verification_v2_at(daa),
        verification_s3_active: p.palw_verification_s3_at(daa),
        verification_s2_active: p.palw_verification_s2_at(daa),
        readiness_v2_active: p.palw_readiness_v2_at(daa),
        attn_anchored_root_active: p.palw_attn_anchored_root_active_at(daa),
        audit_2026_09_11_active: p.palw_audit_2026_09_11_active_at(daa),
        audit_2026_09_11_deep_active: p.palw_audit_2026_09_11_deep_active_at(daa),
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(daa),
        settled_anchor_depth: if p.palw_audit_2026_09_23_active_at(daa) { p.palw_settled_anchor_depth } else { None },
        // **ADR-0152 v2 F2's `palw_offence_attribution` is held dormant here, deliberately.** The
        // suites that include this fixture file the V1 `PanelFalseValid` on testnet-12 to pin the
        // 2026-09-24 audit's fixes to the V1 route (#7, #8, the forged equivocation, the whole-
        // collateral branch) — the fold BELOW that fence, byte for byte — and past it the V1 kind
        // is refused by name. F2's own suites arm it explicitly
        // (`p.palw_offence_attribution_active_at(daa)`), which is what the processor resolves.
        offence_attribution_active: false,
        // ADR-0152 X7 / N9: the processor passes P2-7's constant and nothing else, so the fixture
        // folds with it too (signer liability armed past `palw_rcore_plus` since P2-7).
        seat_da_answer_landed: kaspa_consensus_core::palw_da_rcore_v1::PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1,
        objective_offence_daa: p.palw_objective_offence_daa(),
        seat_gate_possession_daa: p.palw_seat_gate_possession_daa(),
        model_registry: registry_fold(p, daa),
        ..Default::default()
    }
}

/// `palw_model_registry_fold_at` (processor.rs:8872) rebuilt from core: the globals with the
/// bundle's seat count, the lane's span, and the genesis classes' works from the same three
/// sources `palw_known_model_works_v1` consults (the bundle's carriages, the typed catalog, and —
/// for the floor, which neither carries — the canonical work of its own profile).
pub fn registry_fold(p: &Params, daa: u64) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1> {
    use kaspa_consensus_core::palw_model_registry_v1::*;
    if !p.palw_model_registry_at(daa) {
        return None;
    }
    let lane = p.palw_execution_lane_at(daa)?;
    let b = bundle(p);
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = b.panel.seat_count();
    let mut works = palw_genesis_model_works_v1(&b.genesis_objects);
    for (id, w) in palw_rc_typed_class_works_v1() {
        works.entry(id).or_insert(w);
    }
    let floor = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
        .expect("floor profile");
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&floor, pf, dc);
    if let Some(w) = palw_model_work_from_carriage_v1(&floor, &job) {
        works.entry(b.base_class_id).or_insert(w);
    }
    let span = lane.schedule_span_daa_at(daa);
    let activation = p.palw_model_registry.map(|f| f.daa_score()).unwrap_or(0);
    Some(PalwModelRegistryFoldV1 {
        globals,
        span_daa: span,
        genesis_works: works,
        grace_until_daa: PalwModelRegistryFoldV1::grace_until_v1(activation, span, &globals),
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(daa),
    })
}

/// One chain block through the real fold, with the t12 flags and extras at `daa`.
pub fn fold(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error> {
    let f = flags(p, ctx.daa_score);
    apply_palw_transition_v7(
        parent,
        sp,
        None,
        ctx,
        objects,
        work,
        &[],
        exec_key,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &extras(p, ctx.daa_score),
    )
}

pub fn ctx(block: u64, daa: u64, blue: u64, subsidy: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy }
}

/// testnet-12's genesis state: the bundle's registration list folded at DAA 0 exactly as
/// `process_genesis` folds it.
pub fn genesis_state(p: &Params) -> PalwChainStateV2 {
    let b = bundle(p);
    let (s, _, _) = fold(p, &b.state, &PalwChainStateV2::genesis(), &ctx(0xB10C_0000, 0, 0, 0), &b.genesis_objects, PalwBlockWorkV3::None, Hash64::default())
        .expect("the t12 genesis list folds");
    s
}

/// `(class_id, declared leaves, initial target, slash/pwu)` of every genesis class, floor first.
pub fn genesis_classes(p: &Params) -> Vec<(Hash64, u64, u128, u64)> {
    let b = bundle(p);
    let mut rows = Vec::new();
    for o in b.genesis_objects.iter() {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, initial_target, slash_value_per_pwu, .. } = o {
            let leaves = match pwu_rule {
                PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
                PalwPwuRuleV2::MaxPerAttempt(m) => *m,
                #[allow(unreachable_patterns)]
                _ => 0,
            };
            rows.push((*class_id, leaves, *initial_target, *slash_value_per_pwu));
        }
    }
    rows.sort_by_key(|r| (r.0 != b.base_class_id, r.1));
    rows
}

/// The genesis bonds' keys and operator ids, in registry order.
pub fn genesis_bonds(p: &Params) -> Vec<(PalwBondKeyV2, Hash64, u64)> {
    let b = bundle(p);
    b.genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, operator_pubkey, collateral, .. } => {
                Some((*bond, palw_operator_id_v2(operator_pubkey), *collateral))
            }
            _ => None,
        })
        .collect()
}

pub fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xA77A_0000 + n), index: 0 })
}

pub fn pubkey_of(n: u64) -> Vec<u8> {
    let mut v = vec![0xA7u8; 32];
    v[..8].copy_from_slice(&n.to_le_bytes());
    v
}

pub fn operator_pubkey_of(n: u64) -> Vec<u8> {
    let mut v = vec![0x0Bu8; 32];
    v[..8].copy_from_slice(&n.to_le_bytes());
    v
}

/// An attacker / newcomer bond registration (`BondRegistered`), signature checked by acceptance.
/// The collateral a bond must post to register at DAA `daa` on `p` —
/// `palw_bond_registration_floor_v1` over the bundle's `min_collateral_sompi` (the PRODUCER floor
/// since the t12 merge: 13,000 MSK on testnet-12's regenesis params).
pub fn registration_floor(p: &Params, daa: u64) -> u64 {
    kaspa_consensus_core::palw_state_v2::palw_bond_registration_floor_v1(
        bundle(p).state.min_collateral_sompi(),
        p.palw_audit_2026_09_23_active_at(daa),
    )
}

/// `collateral`, or the registration floor where that is higher. For a fixture whose attacker or
/// registrant posted a round amount the pre-merge floors (400,000 / 4,000,000 sompi) admitted; the
/// quantity under test (a reservation, a charge, a forfeiture) is measured on the bond, not
/// derived from its size.
pub fn at_least_the_floor(p: &Params, collateral: u64) -> u64 {
    collateral.max(registration_floor(p, 0))
}

pub fn bond_obj(n: u64, collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey_of(n),
        operator_pubkey: operator_pubkey_of(n),
        collateral,
        payout_payload: h(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// A **junk** attempt: well-formed, signed-length, every root a made-up hash. The fold checks no
/// root against an execution (that is the panel's job), and the class lottery that sits OUTSIDE the
/// fold keys the ticket on the execution commitment, whose preimage here is `seed` — so a new draw
/// is a new `seed`, i.e. one BLAKE2b, not one inference.
pub fn junk_attempt(
    class_id: Hash64,
    bond: PalwBondKeyV2,
    bond_pubkey: Vec<u8>,
    operator_pubkey: &[u8],
    pwu: u64,
    seed: u64,
    pre_pow: u64,
) -> (PalwAttemptEnvelopeV2, Hash64, Hash64) {
    let nonce = 7u64;
    let ts = 1_700_000_000u64 + seed;
    let attempt = PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain: h(NET),
        challenge: challenge_v2(h(NET), h(pre_pow), ts, nonce, class_id, &bond.0),
        class_id,
        executor_bond: bond.0,
        executor_pubkey: bond_pubkey,
        operator_id: palw_operator_id_v2(operator_pubkey),
        artifact_root: h(0xA27),
        trace_root: h(0x1701_0000_0000 ^ seed),
        output_root: h(0x1702_0000_0000 ^ seed),
        pwu,
        trace_manifest_root: attempt_trace_manifest_root_v1(h(0x1701_0000_0000 ^ seed), PALW_ATTEMPT_V2_TRACE_CHUNKS),
        trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: 999_999,
        execution_root: h(0x1703_0000_0000 ^ seed),
    };
    let env = PalwAttemptEnvelopeV2 { attempt, signature: vec![0u8; MLDSA87_SIGNATURE_LEN] };
    let anchor = execution_anchor_v3(h(NET), h(pre_pow), class_id, &bond.0, nonce);
    let key = execution_commitment_v3(&env.attempt, anchor);
    let id = attempt_id_v2(&env.attempt);
    (env, key, id)
}

pub fn valid_receipts(claim: Hash64, seats: &[PalwBondKeyV2]) -> Vec<PalwSeatReceiptV2> {
    seats
        .iter()
        .map(|s| PalwSeatReceiptV2 { claim, verdict: PalwReceiptVerdictV2::Valid, seat_bond: *s, signed_daa: 0, signature: Vec::new() })
        .collect()
}

pub fn seats_of(bonds: &[(PalwBondKeyV2, Hash64)]) -> Vec<PalwPanelSeatV2> {
    bonds.iter().map(|(b, o)| PalwPanelSeatV2 { bond: *b, operator_id: *o }).collect()
}

/// Bytes a syncing peer is sent and every node persists for this state (the carriage).
pub fn carriage_bytes(s: &PalwChainStateV2) -> usize {
    borsh::to_vec(&PalwStateCarriageV2::from_state(s)).expect("carriage serializes").len()
}

/// A deterministic 64-bit generator (splitmix64) — fixed seeds, no dependency.
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// **What the audit's #9 (b38356fe, `AttemptExposureCeiling`) admits per concurrent attempt,
/// read off the runtime.** `apply_attempt` refuses an attempt whose escrow-inclusive reservation
/// (`claim.reserved + claim_escrow_reservation_v1`) would take its bond past
/// `collateral × fp_max_exposure_ratio_permille / 1000` on the live state. One probe attempt of
/// `class` (at `pwu`, carrying `subsidy`) is folded on testnet-12's genesis for a bond that can back
/// it; what the bond reserved for it is the per-attempt reservation, and the collateral that admits
/// one more such attempt is `ceil(reservation × 1000 / ratio)`.
#[derive(Clone, Copy, Debug)]
pub struct AdmittedPerAttempt {
    /// `claim.reserved` — the weight term.
    pub reserved: u128,
    /// Option A's escrow term the bond carries next to it.
    pub escrow: u128,
    /// `reserved + escrow`: what one concurrent attempt holds on the bond, and what `void_and_slash`
    /// takes past the audit fence on a `CourtFraud`, a `ProducerWithholding` or a second
    /// `ReceiptTimeout` (#10).
    pub reservation: u128,
    /// `fp_max_exposure_ratio_permille`.
    pub ratio_permille: u32,
    /// `ceil(reservation × 1000 / ratio)`: the collateral one concurrent attempt needs past #9.
    pub collateral: u64,
}

pub fn admitted_per_attempt(p: &Params, class: Hash64, pwu: u64, subsidy: u64) -> AdmittedPerAttempt {
    admitted_per_attempt_on(p, &genesis_state(p), class, pwu, subsidy)
}

/// [`admitted_per_attempt`] on a given parent state (a state where `class` admits).
pub fn admitted_per_attempt_on(p: &Params, parent: &PalwChainStateV2, class: Hash64, pwu: u64, subsidy: u64) -> AdmittedPerAttempt {
    const PROBE: u64 = 0xAD_0009;
    let b = bundle(p);
    let sp = &b.state;
    let rich = 1_000_000_000_000_000u64; // 10,000,000 MSK: enough to back any genesis class's claim
    let (s, _, _) = fold(p, sp, parent, &ctx(0xAD00_0001, 1_000, 1, 0), &[bond_obj(PROBE, rich)], PalwBlockWorkV3::None, Hash64::default())
        .expect("the probe bond registers");
    let (env, key, id) = junk_attempt(class, bond_key(PROBE), pubkey_of(PROBE), &operator_pubkey_of(PROBE), pwu, 0xAD_5EED, 0xAD_0000_5EED);
    let (s2, _, skips) = fold(p, sp, &s, &ctx(0xAD00_0002, 1_001, 2, subsidy), &[], PalwBlockWorkV3::Attempt(&env), key)
        .expect("the probe attempt folds");
    assert!(skips.is_empty(), "a 10,000,000 MSK bond backs one attempt: {skips:?}");
    let claim = s2.claim(&id).expect("the probe claim is recorded");
    let reserved = claim.reserved;
    let escrow = sp.claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward);
    let reservation = s2.reserved_exposure(&bond_key(PROBE)) - s.reserved_exposure(&bond_key(PROBE));
    assert_eq!(reservation, reserved + escrow, "the bond carries weight + escrow for one attempt (option A)");
    let ratio_permille = sp.fp_max_exposure_ratio_permille();
    let collateral = (reservation * 1000).div_ceil(u128::from(ratio_permille));
    AdmittedPerAttempt { reserved, escrow, reservation, ratio_permille, collateral: u64::try_from(collateral).expect("fits a bond") }
}

/// [`fold`] with the 2026-09-23 audit fence forced OFF — the pre-fence rules (#9's live-state
/// ceiling and #10's withholding charge absent), for PRE-FENCE DEFECT RECORDs only.
pub fn fold_pre_fence(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error> {
    let f = flags(p, ctx.daa_score);
    let mut x = extras(p, ctx.daa_score);
    x.audit_2026_09_23_active = false;
    x.settled_anchor_depth = None;
    apply_palw_transition_v7(
        parent,
        sp,
        None,
        ctx,
        objects,
        work,
        &[],
        exec_key,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &x,
    )
}
