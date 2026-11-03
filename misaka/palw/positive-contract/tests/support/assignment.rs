//! Fixed contract DATA fixtures only; no accepted native state or resources.
use misaka_palw_positive_contract::{assignment::*, identity::*, rows::*};

pub fn d(n: u8) -> Digest {
    Digest([n; 64])
}
pub fn bond(n: u8) -> NativeOutpoint {
    NativeOutpoint { transaction: NativeHash64([n; 64]), index: n as u32 }
}
pub fn bundle() -> AssignmentAdmissionBundleV2 {
    let terms = AssignmentTermsV2 {
        version: 2,
        revision: 1,
        network: d(1),
        ruleset: d(2),
        branch_anchor: NativeHash64([3; 64]),
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        registered_class: d(4),
        poster: bond(5),
        worker: bond(6),
        prover: bond(7),
        payer: PrincipalId(d(8)),
        refund_payload: d(9),
        poster_nonce: 0,
        derivation_policy: d(10),
        source_policy: d(11),
        reserve_schedule: d(12),
        worker_reward: 100,
        max_assignment_fee: 20,
        max_assignment_capital: 30,
        max_assignment_locked: 200,
        accept_before_daa: 100,
        result_before_daa: 200,
        reserve_until_daa: 500,
    };
    let assignment = terms.id();
    AssignmentAdmissionBundleV2 {
        terms,
        job_context: d(13),
        quote: d(14),
        source_catalog: d(15),
        funding: AssignmentFundingAuthorizationV2 {
            assignment,
            quote: d(14),
            payer: terms.payer,
            funding_output_index: 0,
            refund_payload: terms.refund_payload,
            max_fee: 20,
            max_capital: 30,
            max_reward: 100,
            max_locked: 200,
            admission_expiry: 100,
        },
        future_case: FutureCaseFundingBoundsV2 {
            assignment,
            payer: terms.payer,
            refund_payload: terms.refund_payload,
            worker: terms.worker,
            max_case_fee: 40,
            max_case_capital: 60,
            max_case_locked: 100,
            case_admission_before_daa: 200,
            service_before_daa: 450,
        },
        source_consent: d(16),
        prover_consent: d(17),
        cancellation_policy: d(18),
    }
}
