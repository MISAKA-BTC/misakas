//! **The RFC07 selected-approval adapter** (`finals_read_v1` / `final_export_v1` / `selected_approval_event_v1`), asserted on its own.
//!
//! A kernel Final exports a beacon `AttributedWorkV1` only for a `PaidClaimV2` whose canonical paid Final carries the assigned
//! positive quorum. These tests pin both sides of that sentence:
//!
//! * `none_*`: an optimistic timeout, a Panel-licensed Final and a paid claim without its approved Final export `None` — also when
//!   the gate behind the paid arm would say yes for the same claim id;
//! * `approved_*`: an approved paid claim exports an event whose `claim_final` / `da_satisfied` follow the ONE kernel settlement
//!   predicate (Convicted withdraws the fact, Defaulted withdraws its availability).
//!
//! The paid rows here are written byte for byte as the fold writes them (`palw_claim_challenge_fold_v2`: base index, authority,
//! canonical paid Final); they are the INPUTS of the adapter, which is what is under test. How a real chain reaches them is not.
use super::*;
use crate::palw_claim_challenge_v2::*;
use crate::palw_kernel_settlement_v1::{PalwKernelSettlementV1 as S, palw_kernel_row_settlement_v1};
use misaka_palw_kernel::claim_paid_v2::{PaidClaimConsumerV2, PaidClaimEnrollmentV2, PaidClaimLifetimeV2};
use misaka_palw_kernel::evidence::{EvidenceHeaderV1, SuiteParamsV1, VerificationEvidenceV1};
use misaka_palw_kernel::job::KernelClaimV1;
use misaka_palw_kernel::ledger::{ClaimBodyV1, ClaimRowV1};
use misaka_palw_kernel::lifecycle::{ClaimLifecycleV1, ClaimStateV1, LifecyclePolicyV1};
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::opv::{FinalReceiptV1, FinalStandingV1};
use misaka_palw_kernel::rows::{TABLE_CLAIMS_V1, TABLE_OPV_CLASSES_V1};

const PRODUCER: Digest = [0xC4; 64];
const PAYER: Digest = [0xC5; 64];
/// The base class of the OPV paid claims, and of the Panel-licensed ones.
const OPV_CLASS: Digest = [0xA1; 64];
const PANEL_CLASS: Digest = [0xA2; 64];
const PAID_CLASS: Digest = [0xE1; 64];
const FINAL_DAA: u64 = 60;
const UNTIL: u64 = 500;

fn lifetime() -> PaidClaimLifetimeV2 {
    PaidClaimLifetimeV2 { committed_daa: 10, checking_until: 200, challenge_until: 300, dispute_until: 400, liability_until: UNTIL }
}

fn enrollment(claim: Digest, job: Digest) -> PaidClaimEnrollmentV2 {
    PaidClaimEnrollmentV2 {
        version: 2,
        class: PAID_CLASS,
        job,
        base_claim: claim,
        producer: PRODUCER,
        commit_root: [0xE3; 64],
        lifetime: lifetime(),
        accepted_until_daa: 20,
        material_consent: [0xE4; 64],
        round: 0,
        prior_transcript: [0; 64],
    }
}

fn final_row(e: &PaidClaimEnrollmentV2) -> PalwPaidClaimFinalV2 {
    PalwPaidClaimFinalV2 {
        enrollment: e.id(),
        base_claim: e.base_claim,
        verification_class: e.class,
        sample_seed: [0x51; 64],
        source_context: [0x52; 64],
        signed_lifetime: e.lifetime,
        complete_receipts: 3,
        logical_final_daa: 400,
        completed_rounds: 1,
        transcript_root: [0; 64],
    }
}

fn context_of(claim: &Digest) -> Digest {
    let mut c = *claim;
    c[0] ^= 0xFF;
    c
}

/// The route state the gate reads: the claim row's fixed `producer ‖ class` prefix, and the OPV class marker.
fn route_with_claim(claim: Digest, class: Digest, opv_class: bool, route: &mut PalwKernelRouteStateV1) {
    let mut raw = Vec::new();
    raw.extend_from_slice(&PRODUCER);
    raw.extend_from_slice(&class);
    raw.extend_from_slice(&[0; 8]);
    route.rows.insert((TABLE_CLAIMS_V1, claim.to_vec()), raw);
    if opv_class {
        route.rows.insert((TABLE_OPV_CLASSES_V1, class.to_vec()), Vec::new());
    }
}

/// The two aux rows a paid enrollment writes (`PAID_CLAIM_ENROLLMENTS_V2`): the base index and the authority, whose first bytes are
/// the version and the enrollment (what the gate reads of it).
fn paid_enrollment_rows(route: &mut PalwKernelRouteStateV1, e: &PaidClaimEnrollmentV2) {
    let target = context_of(&e.base_claim);
    let index = PalwPaidClaimIndexV3 { version: 3, kind: PalwPaidClaimIndexKindV3::BaseClaim, owner: e.base_claim, target };
    route.aux.insert(
        (PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(PAID_CLAIM_BASE_INDEX_PREFIX_V3, &e.base_claim)),
        borsh::to_vec(&index).unwrap(),
    );
    let mut authority = borsh::to_vec(&2u16).unwrap();
    authority.extend(borsh::to_vec(e).unwrap());
    authority.extend_from_slice(&[0xFF; 7]); // the rest of the authority is not the gate's business
    route.aux.insert((PAID_CLAIM_ENROLLMENTS_V2, target.to_vec()), authority);
}

fn put_final(route: &mut PalwKernelRouteStateV1, row: &PalwPaidClaimFinalV2) {
    route.aux.insert((PAID_CLAIM_FINALS_V2, row.enrollment.to_vec()), borsh::to_vec(row).unwrap());
}

fn gate(route: &PalwKernelRouteStateV1, claim: Digest) -> bool {
    crate::palw_exec_v2_verify::palw_exec_v2_selected_approval_v1(route, &Hash64::from_bytes(claim))
}

fn empty_route() -> PalwKernelRouteStateV1 {
    let policy = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
    PalwKernelRouteStateV1::new(policy, None, LedgerScalarsV1::default())
}

// ---- the gate ------------------------------------------------------------------------------------------------------------

#[test]
fn none_gate_admits_only_a_paid_claim_with_its_canonical_final_and_a_positive_quorum() {
    let (claim, job) = ([0x11; 64], [0x12; 64]);
    let e = enrollment(claim, job);

    // No claim row at all.
    assert!(!gate(&empty_route(), claim));

    // An OPV claim with no paid enrollment: a timeout is not an approval (the base-index miss falls to the OPV-class check).
    let mut opv = empty_route();
    route_with_claim(claim, OPV_CLASS, true, &mut opv);
    assert!(!gate(&opv, claim), "an optimistic claim never counts as selected-approved without a paid Final");

    // A paid enrollment whose canonical paid Final has not been written (the unapproved paid claim).
    let mut paid = empty_route();
    route_with_claim(claim, OPV_CLASS, true, &mut paid);
    paid_enrollment_rows(&mut paid, &e);
    assert!(!gate(&paid, claim), "no canonical paid Final: not approved");

    // A paid Final that does not carry the whole approval.
    let good = final_row(&e);
    let broken: Vec<(&str, PalwPaidClaimFinalV2)> = vec![
        ("zero receipts", PalwPaidClaimFinalV2 { complete_receipts: 0, ..good.clone() }),
        ("zero completed rounds", PalwPaidClaimFinalV2 { completed_rounds: 0, ..good.clone() }),
        ("another enrollment", PalwPaidClaimFinalV2 { enrollment: [0x99; 64], ..good.clone() }),
        ("another base claim", PalwPaidClaimFinalV2 { base_claim: [0x98; 64], ..good.clone() }),
        ("another verification class", PalwPaidClaimFinalV2 { verification_class: [0x97; 64], ..good.clone() }),
        (
            "another signed lifetime",
            PalwPaidClaimFinalV2 { signed_lifetime: PaidClaimLifetimeV2 { liability_until: UNTIL + 1, ..e.lifetime }, ..good.clone() },
        ),
    ];
    for (why, row) in broken {
        let mut r = paid.clone();
        // A Final filed under the enrollment's own key (what a substitution would have to do).
        r.aux.insert((PAID_CLAIM_FINALS_V2, e.id().to_vec()), borsh::to_vec(&row).unwrap());
        assert!(!gate(&r, claim), "{why}: not approved");
    }

    // A malformed base index.
    for index in [
        PalwPaidClaimIndexV3 { version: 2, kind: PalwPaidClaimIndexKindV3::BaseClaim, owner: claim, target: context_of(&claim) },
        PalwPaidClaimIndexV3 { version: 3, kind: PalwPaidClaimIndexKindV3::Enrollment, owner: claim, target: context_of(&claim) },
        PalwPaidClaimIndexV3 { version: 3, kind: PalwPaidClaimIndexKindV3::BaseClaim, owner: [0x10; 64], target: context_of(&claim) },
    ] {
        let mut r = paid.clone();
        put_final(&mut r, &good);
        r.aux.insert(
            (PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(PAID_CLAIM_BASE_INDEX_PREFIX_V3, &claim)),
            borsh::to_vec(&index).unwrap(),
        );
        assert!(!gate(&r, claim), "{index:?}: not approved");
    }

    // The approved paid claim.
    let mut approved = paid.clone();
    put_final(&mut approved, &good);
    assert!(gate(&approved, claim), "the canonical paid Final with its quorum and a completed round");
}

// ---- the export ------------------------------------------------------------------------------------------------------------

/// One Final claim row of `class` in the ledger. `reserved`/`liability` are the settlement predicate's inputs.
fn claim_row(class: Digest, job: Digest, reserved: u64, convicted: bool) -> ClaimRowV1 {
    ClaimRowV1 {
        producer: PRODUCER,
        class_binding_id: class,
        job_id: job,
        body: ClaimBodyV1::Program {
            claim: KernelClaimV1 { job_id: job, producer_bond: PRODUCER, generated: vec![1], evidence_root: [0x77; 64] },
            evidence: VerificationEvidenceV1 {
                version: 1,
                header: EvidenceHeaderV1 {
                    network_domain: [1; 64],
                    ruleset_digest: [2; 64],
                    class_binding_id: class,
                    program_root: [3; 64],
                    artifact_root: [4; 64],
                    plan_root: [5; 64],
                },
                job_input_root: [6; 64],
                positions: 1,
                initial_state_root: [7; 64],
                final_state_root: [8; 64],
                trace_root: [9; 64],
                output_root: [10; 64],
                segments: Vec::new(),
                suite: SuiteParamsV1 {
                    descriptor_digest: [11; 64],
                    checker_suite_id: 1,
                    challenge_policy_id: 1,
                    repetitions: 1,
                    target_bits: 1,
                    field_bits: 1,
                },
            },
            commitments: Vec::new(),
        },
        committed_daa: 10,
        life: ClaimLifecycleV1 {
            policy: LifecyclePolicyV1 { check_window_daa: 10, challenge_window_daa: 10 },
            state: ClaimStateV1::Final { final_daa: FINAL_DAA },
            retention_met: true,
            final_hold_until: 0,
        },
        reserved,
        liability_until: Some(UNTIL),
        convicted,
        rewarded: false,
        sealed_daa: 0,
    }
}

struct World {
    route: PalwKernelRouteStateV1,
    ledger: crate::palw_native_resource_v1::ReservedKernelLedgerV1,
}

impl World {
    fn new() -> World {
        let route = empty_route();
        let mut ledger = route.ledger_checked_v1().expect("an empty ledger");
        // The OPV class marker lives in the ledger; the route's rows (the gate's input) get theirs from `add`.
        ledger.preview_mut_v1().opv.classes.insert(OPV_CLASS);
        World { route, ledger }
    }

    /// A Final claim of `class` on a fresh job, `paid` as a (possibly unapproved) paid enrollment. Returns its id.
    fn add(&mut self, n: u8, class: Digest, paid: Option<bool>, reserved: u64, convicted: bool) -> Digest {
        let (claim, job) = ([n; 64], [n.wrapping_add(0x40); 64]);
        let opv = class == OPV_CLASS;
        route_with_claim(claim, class, opv, &mut self.route);
        let l = self.ledger.preview_mut_v1();
        l.claims.insert(claim, claim_row(class, job, reserved, convicted));
        l.job_posters.insert(job, PAYER);
        if let Some(approved) = paid {
            let e = enrollment(claim, job);
            l.paid_claim_consumers_v2.insert(claim, PaidClaimConsumerV2 { lifetime: e.lifetime, complete: approved });
            paid_enrollment_rows(&mut self.route, &e);
            if approved {
                put_final(&mut self.route, &final_row(&e));
            }
        }
        claim
    }

    /// Exactly what `finals_read_v1` does for one claim, at `daa`: the receipt, the settlement predicate, the export decision.
    fn export(
        &mut self,
        claim: &Digest,
        daa: u64,
    ) -> (FinalReceiptV1, S, &'static str, Option<misaka_palw_challenge::AttributedWorkV1>) {
        self.ledger.set_daa_v1(daa);
        let receipt = self.ledger.final_receipt(claim).expect("a Final claim");
        let settlement = palw_kernel_row_settlement_v1(&self.ledger, claim, daa).unwrap();
        let (path, event) = self.route.final_export_v1(&self.ledger, &receipt, settlement).expect("the export decision");
        (receipt, settlement, path, event.map(|b| borsh::from_slice(&b).expect("a beacon event")))
    }
}

#[test]
fn none_timeout_panel_licensed_and_unapproved_paid_claims_export_no_event() {
    let mut w = World::new();
    // (1) an optimistic Final: the dispute window merely expired.
    let timeout = w.add(1, OPV_CLASS, None, 10, false);
    // (2) a Panel-licensed Final of an unpaid claim. The GATE would say yes for this claim id (a non-OPV class with no paid index —
    // the Panel route's native tally is EXEC's approval), and the export still does not read it: the route does not know the
    // licensing Panel's seed and epoch.
    let panel = w.add(2, PANEL_CLASS, None, 10, false);
    assert!(gate(&w.route, panel), "the precondition: EXEC's gate alone would admit this Panel-licensed claim");
    // (3) a paid claim over an OPV class whose canonical paid Final was never written.
    let unapproved = w.add(3, OPV_CLASS, Some(false), 10, false);
    // (4) the same with the Final row's quorum emptied.
    let hollow = w.add(4, OPV_CLASS, Some(true), 10, false);
    let e = enrollment(hollow, [4u8.wrapping_add(0x40); 64]);
    put_final(&mut w.route, &PalwPaidClaimFinalV2 { complete_receipts: 0, ..final_row(&e) });
    // (5) the same with no completed round.
    let no_round = w.add(5, OPV_CLASS, Some(true), 10, false);
    let e = enrollment(no_round, [5u8.wrapping_add(0x40); 64]);
    put_final(&mut w.route, &PalwPaidClaimFinalV2 { completed_rounds: 0, ..final_row(&e) });

    let expect = [
        (timeout, "OptimisticUnapproved", VerificationModeV1::OptimisticPublicVerification),
        (panel, "PanelLicensed", VerificationModeV1::PanelLicensed),
        (unapproved, "PaidClaimV2", VerificationModeV1::OptimisticPublicVerification),
        (hollow, "PaidClaimV2", VerificationModeV1::OptimisticPublicVerification),
        (no_round, "PaidClaimV2", VerificationModeV1::OptimisticPublicVerification),
    ];
    for (claim, path, mode) in expect {
        let (receipt, _, got_path, event) = w.export(&claim, 70);
        assert_eq!((got_path, receipt.mode), (path, mode));
        assert!(event.is_none(), "{path}: no beacon event without selected approval");
    }
}

#[test]
fn none_a_paid_claim_over_a_panel_licensed_class_exports_none_and_does_not_fail_the_read() {
    let mut w = World::new();
    let claim = w.add(6, PANEL_CLASS, Some(true), 10, false);
    let (receipt, settlement, path, event) = w.export(&claim, 70);
    assert!(gate(&w.route, claim) && matches!(settlement, S::FinalRevocable { .. }), "approved, and a standing Final");
    assert_eq!((path, receipt.mode), ("PaidClaimV2", VerificationModeV1::PanelLicensed));
    // The precondition of the bug this pins: the kernel refuses a Panel-licensed event that does not name its Panel …
    let ctx = misaka_palw_kernel::opv::WorkFinalContextV1 {
        accepted_position: 10,
        settlement_position: FINAL_DAA,
        occurrence_index: 0,
        validity_independent: true,
        depends_on_profiles: Vec::new(),
        panel: None,
    };
    assert!(receipt.to_work_final_event(&ctx).is_err());
    // … and the adapter answers that with no event, where it used to propagate the refusal and fail every Final read.
    assert!(event.is_none());
    assert_eq!(
        selected_approval_event_v1(&w.ledger, &receipt, settlement),
        Ok(None),
        "a Panel-licensed receipt is not an error of the read"
    );
}

#[test]
fn approved_paid_claim_exports_an_event_that_follows_the_settlement_predicate() {
    let mut w = World::new();
    // The four standings the ONE predicate distinguishes, all of approved paid claims of an OPV class.
    let standing = w.add(10, OPV_CLASS, Some(true), 10, false); // reservation held, inside the horizon → final-revocable
    let convicted = w.add(11, OPV_CLASS, Some(true), 0, true); // convicted after Final
    let defaulted = w.add(12, OPV_CLASS, Some(true), 0, false); // the reservation is gone while still liable
    let settled = w.add(13, OPV_CLASS, Some(true), 0, false); // past the liability horizon

    let (receipt, s, path, event) = w.export(&standing, 70);
    let event = event.expect("an approved paid Final exports its event");
    assert_eq!((path, s), ("PaidClaimV2", S::FinalRevocable { final_daa: FINAL_DAA, liability_until: UNTIL }));
    assert_eq!(receipt.standing, FinalStandingV1::Standing);
    assert!(event.event.claim_final && event.event.da_satisfied, "a standing Final keeps both facts");
    assert_eq!(event.event.final_path, misaka_palw_challenge::FinalPathV1::PanelIndependent);
    assert!(event.event.validity_independent);
    assert_eq!(event.event.settlement_position, FINAL_DAA);
    assert_eq!(event.event.accepted_position, 10, "the earliest commitment of the job");
    assert_eq!(event.event.source_profile_id, OPV_CLASS);
    assert_eq!(event.attribution.producer_id, PRODUCER);
    assert_eq!(
        event.attribution.consumer_id,
        misaka_palw_challenge::RootV1::Present(PAYER),
        "the job's permanent payer, from the ledger's rows"
    );

    let (_, s, _, event) = w.export(&convicted, 70);
    let event = event.expect("a convicted paid Final still reads (its fact is withdrawn, not hidden)");
    assert_eq!(s, S::Convicted);
    assert!(!event.event.claim_final, "Convicted → claim_final false");

    let (_, s, _, event) = w.export(&defaulted, 70);
    let event = event.expect("a defaulted paid Final still reads");
    assert_eq!(s, S::Defaulted);
    assert!(
        event.event.claim_final && !event.event.da_satisfied,
        "Defaulted → da_satisfied false (the fact stands, its material does not)"
    );

    let (_, s, _, event) = w.export(&settled, UNTIL + 1);
    let event = event.expect("a settled paid Final exports its event");
    assert!(matches!(s, S::Settled { .. }), "{s:?}");
    assert!(event.event.claim_final && event.event.da_satisfied);

    // The same row facts, one DAA before the horizon: the reservation is gone while liable → Defaulted; no other input changed.
    let (_, s, _, event) = w.export(&settled, UNTIL);
    assert_eq!(s, S::Defaulted);
    assert!(!event.expect("it reads").event.da_satisfied);
}
