//! **RFC-0016 V8 through the actual fold** (audit 2026-10-17, 16-A): the capacity attestation carrier (kernel inner 76) folds into
//! kernel-route aux 135 `0x05`, is replaced by a re-attestation, revoked by an all-zero vector, refused below the fence / unsigned /
//! without evidence / for an unregistered class, pruned at its expiry, restored by a reorg (delta revert), and the capacity read past
//! the fence takes its supply from the stored rows: lower capability or expiry lowers the admitted amount, one machine attested to
//! two classes is one budget, compute above the reference seat is not credited, and an unmeasured per-claim load credits nothing.
//!
//! Every value here is a TEST fixture (the measured per-claim load included), never a proposal.
use super::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use crate::palw_rfc0016_v1::values::{PalwRfc0016ClassLoadV1, PalwRfc0016MirrorV1, PalwRfc0016ValuesV1};
use crate::palw_state_v2::palw_rfc0016_fold_v1::{palw_rfc0016_attested_capacity_v1, palw_rfc0016_court_bucket_bound_v1};
use misaka_palw_kernel::capacity_attestation_v1::{CAPACITY_ATTESTATION_VERSION_V1, CapacityAttestationV1};
use misaka_palw_kernel::route::KernelRouteObjectV1;

const FENCE: u64 = 50;
const BONDS: u64 = 4;
const MODEL_A: u64 = 2;
const MODEL_B: u64 = 3;
/// The reference seat in the demand terms' unit (a fixture figure).
const SEAT: u128 = 1_000 << 32;

/// The per-class measured loads (TEST fixture): both classes 1,000 DA bytes and 100 court units per claim.
static LOADS: [PalwRfc0016ClassLoadV1; 2] = [
    PalwRfc0016ClassLoadV1 { class_id: word(MODEL_A), da_bytes_per_claim: 1_000, court_units_per_claim: 100 },
    PalwRfc0016ClassLoadV1 { class_id: word(MODEL_B), da_bytes_per_claim: 1_000, court_units_per_claim: 100 },
];

/// `h64(v).as_bytes()` as a const (the word is the last eight bytes, little-endian).
const fn word(v: u64) -> [u8; 64] {
    let b = v.to_le_bytes();
    let mut out = [0u8; 64];
    let mut i = 0;
    while i < 8 {
        out[56 + i] = b[i];
        i += 1;
    }
    out
}

fn load() -> &'static [PalwRfc0016ClassLoadV1] {
    &LOADS
}

fn armed(claim_loads: &'static [PalwRfc0016ClassLoadV1]) -> PalwStateParamsV2 {
    params().with_rfc0016_v1(Some(
        PalwRfc0016MirrorV1 {
            rights_from_daa: None,
            capacity_from_daa: Some(FENCE),
            values: PalwRfc0016ValuesV1::default(),
            claim_loads: &[],
        }
        .with_claim_loads(claim_loads),
    ))
}

fn extras() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: h64(10),
            ruleset_digest: h64(11),
            chain_genesis: h64(12),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn step_delta(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    apply_palw_transition_v2_with_extras(parent, p, &ctx(daa, daa, daa), objects, None, false, false, false, false, &extras())
}

fn step(parent: &PalwChainStateV2, p: &PalwStateParamsV2, daa: u64, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    step_delta(parent, p, daa, objects).map(|(s, _)| s)
}

fn kid(bond: u64) -> [u8; 64] {
    palw_kernel_bond_id_v1(&bond_key(bond))
}

fn attestation(verifier: u64, class: u64, compute: u128, da: u64, court: u64, expires: u64) -> CapacityAttestationV1 {
    CapacityAttestationV1 {
        version: CAPACITY_ATTESTATION_VERSION_V1,
        network: h64(10).as_bytes(),
        verifier: kid(verifier),
        class_id: h64(class).as_bytes(),
        compute_ccu_per_span: compute,
        memory_bytes: 1 << 30,
        da_bytes_per_span: da,
        court_permille_per_span: court,
        window_from_daa: 1,
        window_to_daa: FENCE,
        expires_daa: expires,
        evidence_root: h64(0xE0).as_bytes(),
    }
}

fn attest(signer: u64, a: CapacityAttestationV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::KernelRouteV1 {
        bytes: KernelRouteObjectV1::AttestCapacityV1 { attestation: a }.encode(),
        signer: bond_key(signer),
        signature: Vec::new(),
    }
}

fn lifecycle(ccu: u128, window: u32) -> crate::palw_model_registry_v1::PalwModelLifecycleRowV1 {
    crate::palw_model_registry_v1::PalwModelLifecycleRowV1 {
        state: crate::palw_model_registry_v1::PalwModelLifecycleV1::Active,
        work: crate::palw_model_registry_v1::PalwModelWorkV1 {
            verification_ccu: ccu,
            economic_ccu_per_claim: ccu,
            working_set_bytes: 1 << 20,
            ops_supported: true,
            ..Default::default()
        },
        profile: crate::palw_model_registry_v1::PalwDerivedProfileV1 { verification_window_spans: window, ..Default::default() },
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    }
}

/// The floor, four bonds and two registered model classes (A: 10 CCU per claim over 10 spans, B: 20 CCU over 10 spans), with the
/// kernel route header in place.
fn world(p: &PalwStateParamsV2) -> PalwChainStateV2 {
    let mut objects = vec![registration(h64(1), 1000, None)];
    for b in 1..=BONDS {
        objects.push(bond_registration_with_collateral(b, b as u8, 100 + b, 1_000));
    }
    let mut s = step(&PalwChainStateV2::genesis(), p, 1, &objects).expect("the floor and the bonds register");
    s.model_lifecycles.insert(h64(MODEL_A), lifecycle(10, 10));
    s.model_lifecycles.insert(h64(MODEL_B), lifecycle(20, 10));
    s.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(10), h64(11)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: FENCE, burned: 0 },
    ));
    s
}

/// `(capacity, demand, ready)` of `class` under `terms`, every verifier ready.
fn read(s: &PalwChainStateV2, p: &PalwStateParamsV2, class: u64, now: u64, terms: &BTreeMap<Hash64, u128>) -> (u128, u128, u32) {
    palw_rfc0016_attested_capacity_v1(s, &h64(class), now, terms, SEAT, &p.rfc0016_v1().expect("armed"), |_, _| true)
}

/// Demand of `claims` claims of `class` (one seat), in the terms' unit.
fn demand(s: &PalwChainStateV2, class: u64, claims: u128) -> u128 {
    let row = &s.model_lifecycles[&h64(class)];
    crate::palw_work_target_v1::palw_panel_demand_term_v1(claims, row.work.economic_ccu_per_claim, row.profile.verification_window_spans as u64)
}

#[test]
fn rfc16_v8_attestation_folds_replaces_revokes_and_is_refused_where_it_must_be() {
    let p = armed(load());
    let s = world(&p);
    let now = FENCE + 10;
    // Refusals: below the fence, a verifier that is not the signer, no evidence, an open window, an unregistered class, a revocation
    // of nothing, a Retiring-free wrong network.
    assert!(step(&s, &p, FENCE - 1, &[attest(1, attestation(1, MODEL_A, 100, 10_000, 1_000, 1_000))]).is_err(), "below the fence");
    assert!(step(&s, &p, now, &[attest(2, attestation(1, MODEL_A, 100, 10_000, 1_000, 1_000))]).is_err(), "not its signer");
    let no_evidence = CapacityAttestationV1 { evidence_root: [0; 64], ..attestation(1, MODEL_A, 100, 10_000, 1_000, 1_000) };
    assert!(step(&s, &p, now, &[attest(1, no_evidence)]).is_err(), "no evidence");
    let open = CapacityAttestationV1 { window_to_daa: now + 1, ..attestation(1, MODEL_A, 100, 10_000, 1_000, 1_000) };
    assert!(step(&s, &p, now, &[attest(1, open)]).is_err(), "the measurement window is not closed");
    assert!(step(&s, &p, now, &[attest(1, attestation(1, 9, 100, 10_000, 1_000, 1_000))]).is_err(), "unregistered class");
    assert!(step(&s, &p, now, &[attest(1, attestation(1, 1, 100, 10_000, 1_000, 1_000))]).is_err(), "the floor class");
    let other_net = CapacityAttestationV1 { network: h64(99).as_bytes(), ..attestation(1, MODEL_A, 100, 10_000, 1_000, 1_000) };
    assert!(step(&s, &p, now, &[attest(1, other_net)]).is_err(), "another network");
    let revoke_nothing = attestation(1, MODEL_A, 0, 0, 0, 1_000);
    let revoke_nothing = CapacityAttestationV1 { memory_bytes: 0, ..revoke_nothing };
    assert!(step(&s, &p, now, &[attest(1, revoke_nothing.clone())]).is_err(), "nothing to revoke");

    // Folds; the read API names it with the verifier's bond.
    let (s1, delta) = step_delta(&s, &p, now, &[attest(1, attestation(1, MODEL_A, 100, 10_000, 1_000, 1_000))]).expect("folds");
    let rows = s1.rfc0016_capacity_attestations_v1();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].verifier, rows[0].model, rows[0].capability.compute_ccu_per_span), (bond_key(1), h64(MODEL_A), 100));
    // Reorg: the delta reverts the row bit for bit.
    assert_eq!(revert_delta_v2(&s1, &delta, &p).unwrap(), s, "a reorg restores the pre-attestation state");
    // A re-attestation replaces (one row per (class, verifier)); a revocation removes it.
    let s2 = step(&s1, &p, now + 1, &[attest(1, attestation(1, MODEL_A, 40, 10_000, 1_000, 1_000))]).expect("re-attests");
    let rows = s2.rfc0016_capacity_attestations_v1();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].capability.compute_ccu_per_span, 40);
    let s3 = step(&s2, &p, now + 2, &[attest(1, revoke_nothing)]).expect("revokes");
    assert!(s3.rfc0016_capacity_attestations_v1().is_empty());
    // Expiry: the tick prunes it at its expiry DAA.
    let s4 = step(&s1, &p, 999, &[]).expect("a block");
    assert_eq!(s4.rfc0016_capacity_attestations_v1().len(), 1);
    let s5 = step(&s4, &p, 1_000, &[]).expect("the expiry block");
    assert!(s5.rfc0016_capacity_attestations_v1().is_empty(), "pruned at expiry");
}

#[test]
fn rfc16_v8_the_capacity_read_follows_the_attestations() {
    let p = armed(load());
    let s = world(&p);
    let now = FENCE + 10;
    let terms: BTreeMap<Hash64, u128> = [(h64(MODEL_A), demand(&s, MODEL_A, 50))].into_iter().collect();
    // No attestation: nothing is credited.
    assert_eq!(read(&s, &p, MODEL_A, now, &terms).0, 0);
    // One verifier, compute-bound: 100 CCU per span against A's 1 CCU-per-span claims (10 over 10 spans) → 100 claims' worth.
    let rich = step(&s, &p, now, &[attest(1, attestation(1, MODEL_A, 100, 1 << 40, 1 << 40, 1_000))]).expect("folds");
    let (cap, d, ready) = read(&rich, &p, MODEL_A, now, &terms);
    assert_eq!(ready, 1);
    assert!(cap >= d, "100 claims' compute serves a 50-claim demand: {cap} vs {d}");
    // The same verifier's lower capability lowers it: DA 2,000 bytes per span against 100 bytes per claim per span → 20 claims.
    let low = step(&s, &p, now, &[attest(1, attestation(1, MODEL_A, 100, 2_000, 1 << 40, 1_000))]).expect("folds");
    let (cap_low, ..) = read(&low, &p, MODEL_A, now, &terms);
    assert!(cap_low < d && cap_low <= demand(&s, MODEL_A, 20), "DA binds: {cap_low}");
    // Court binds the same way: 50 permille per span against 10 per claim per span → 5 claims.
    let court = step(&s, &p, now, &[attest(1, attestation(1, MODEL_A, 100, 1 << 40, 50, 1_000))]).expect("folds");
    assert!(read(&court, &p, MODEL_A, now, &terms).0 <= demand(&s, MODEL_A, 5));
    // Expired: nothing (the read refuses an attestation past its expiry even before the tick prunes it).
    assert_eq!(read(&rich, &p, MODEL_A, 1_000, &terms).0, 0);
    // Compute above the reference seat is not credited: declaring 10^9 CCU gives at most the seat.
    let liar = step(&s, &p, now, &[attest(1, attestation(1, MODEL_A, 1_000_000_000, 1 << 40, 1 << 40, 1_000))]).expect("folds");
    assert!(read(&liar, &p, MODEL_A, now, &terms).0 <= SEAT);
    // One machine attested to two classes is ONE budget: the two classes' capacities sum to at most what it gives one class alone.
    let both = step(
        &s,
        &p,
        now,
        &[
            attest(1, attestation(1, MODEL_A, 100, 1 << 40, 1 << 40, 1_000)),
            attest(1, attestation(1, MODEL_B, 100, 1 << 40, 1 << 40, 1_000)),
        ],
    )
    .expect("folds");
    let two: BTreeMap<Hash64, u128> =
        [(h64(MODEL_A), demand(&s, MODEL_A, 1_000)), (h64(MODEL_B), demand(&s, MODEL_B, 1_000))].into_iter().collect();
    let alone = read(&rich, &p, MODEL_A, now, &two).0;
    let (a, _, _) = read(&both, &p, MODEL_A, now, &two);
    let (b, _, _) = read(&both, &p, MODEL_B, now, &two);
    assert!(a + b <= alone, "one machine, two classes: {a} + {b} <= {alone}");
    // A second machine adds its own budget.
    let two_machines = step(&rich, &p, now + 1, &[attest(2, attestation(2, MODEL_A, 100, 1 << 40, 1 << 40, 1_000))]).expect("folds");
    let (more, _, ready2) = read(&two_machines, &p, MODEL_A, now + 1, &two);
    assert_eq!(ready2, 2);
    assert!(more > alone);
    // Unmeasured per-claim load (the binary's value): nothing is credited, however rich the attestation.
    let unmeasured = armed(&[]);
    assert_eq!(read(&rich, &unmeasured, MODEL_A, now, &terms).0, 0);
}

/// User decision 2026-10-17: the court admission bound past the fence is DERIVED — `claims_per_span = Σ court_units_per_span /
/// court_units_per_claim` over the class's attested machines, times `bucket / span` — and an unmeasured class's bound is 0.
#[test]
fn rfc16_v8_the_court_bucket_bound_is_derived_from_the_attested_court_units() {
    let p = armed(load());
    let s = world(&p);
    let now = FENCE + 10;
    assert_eq!(palw_rfc0016_court_bucket_bound_v1(&s, &params(), &h64(MODEL_A), now, 300, 10), None, "below the fence: no bound");
    assert_eq!(palw_rfc0016_court_bucket_bound_v1(&s, &p, &h64(MODEL_A), now, 300, 10), Some(0), "no attestation");
    let s = step(
        &s,
        &p,
        now,
        &[
            attest(1, attestation(1, MODEL_A, 100, 1 << 40, 1_050, 1_000)),
            attest(2, attestation(2, MODEL_A, 100, 1 << 40, 500, 1_000)),
            attest(3, attestation(3, MODEL_B, 100, 1 << 40, 10_000, 1_000)),
        ],
    )
    .expect("folds");
    // (1,050/100) + (500/100) = 10 + 5 = 15 claims per span; × 300 / 10 = 450 per bucket. Bond 3 attests only B.
    assert_eq!(palw_rfc0016_court_bucket_bound_v1(&s, &p, &h64(MODEL_A), now, 300, 10), Some(450));
    assert_eq!(palw_rfc0016_court_bucket_bound_v1(&s, &p, &h64(MODEL_A), now, 300, 0), Some(0), "no span clock: fail closed");
    assert_eq!(palw_rfc0016_court_bucket_bound_v1(&s, &armed(&[]), &h64(MODEL_A), now, 300, 10), Some(0), "unmeasured class");
}
