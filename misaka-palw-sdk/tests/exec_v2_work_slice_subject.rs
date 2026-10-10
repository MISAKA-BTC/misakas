//! **RFC-0008 v2 against the shared challenge contract (RFC-0007 Part VI)**: the `WORK_SLICE` subject a slice determines is checked
//! here, in a crate that sees both sides — `kaspa-consensus-core` (the slice, `PalwWorkSliceV1::challenge_subject`) and
//! `misaka-palw-challenge` (`ChallengeSubjectV1`, `SubjectKindV1::WorkSlice`) — so the mapping is not restated in either.
//!
//! What must hold, whatever the verification route turns out to be (it is an external gate, spec section 9):
//!
//! * the contract names the kind `WORK_SLICE` and a slice maps field for field onto the subject's typed roots;
//! * **no field of a slice escapes the subject**: flipping any one of the fourteen changes the contract's `ChallengeSubjectV1::id()`, so
//!   a challenge seeded from that subject cannot be replayed against a slice that differs anywhere;
//! * the pre-beacon `commitment_root` binds every field the typed roots do not carry (root claim, index, range, predecessor, output,
//!   DA, job, executor), so it cannot be re-pointed after the beacon;
//! * a work slice is not a beacon source: the contract treats `ExecWorkSlice` as not useful work for its beacon (no entropy from the
//!   lane an attacker can mint blocks on).
//!
//! Run: `cargo test -p misaka-palw-sdk --test exec_v2_work_slice_subject`

use kaspa_consensus_core::palw_exec_v2::{PalwWorkRangeV1, PalwWorkSliceV1};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_challenge::beacon::WorkSourceKindV1;
use misaka_palw_challenge::hash::h as challenge_hash;
use misaka_palw_challenge::{ChallengeSubjectV1, Digest, RootV1, SubjectKindV1};

fn hash(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn digest(hash: &Hash64) -> Digest {
    *hash.as_byte_slice()
}

fn bond(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(v), 0))
}

fn slice() -> PalwWorkSliceV1 {
    PalwWorkSliceV1 {
        root_claim_id: hash(100),
        slice_index: 2,
        class_id: hash(101),
        canonical_job_id: hash(102),
        kernel_version: 3,
        plan_root: hash(103),
        canonical_range: PalwWorkRangeV1 { start: 2_000, end: 3_000 },
        predecessor_state_root: hash(104),
        result_state_root: hash(105),
        input_root: hash(106),
        output_root: hash(107),
        evidence_root: hash(108),
        da_root: hash(109),
        executor_bond: bond(7),
    }
}

/// The contract's subject for `slice`: the half the slice fixes from `PalwWorkSliceSubjectV1`, the other half (network and class
/// roots) fixed here, the kernel root derived from the slice's kernel version the way a test must (the real derivation is the
/// root's class record's).
fn subject_of(slice: &PalwWorkSliceV1) -> ChallengeSubjectV1 {
    let part = slice.challenge_subject();
    ChallengeSubjectV1 {
        chain_genesis: [1; 64],
        ruleset_id: [2; 64],
        challenge_policy_id: [3; 64],
        subject_kind: SubjectKindV1::WorkSlice,
        subject_id: digest(&part.subject_id),
        kernel_id: RootV1::Present(challenge_hash(b"test/kernel", &part.kernel_version.to_le_bytes())),
        verification_plan_root: RootV1::Present(digest(&part.verification_plan_root)),
        program_root: RootV1::Present([4; 64]),
        artifact_root: RootV1::Present([5; 64]),
        tokenizer_or_schema_root: RootV1::Present([6; 64]),
        layout_root: RootV1::Present([7; 64]),
        input_root: RootV1::Present(digest(&part.input_root)),
        state_root: RootV1::Present(digest(&part.state_root)),
        constraint_root: RootV1::Present(digest(&part.constraint_root)),
        commitment_root: digest(&part.commitment_root),
    }
}

#[test]
fn the_contract_names_the_kind_and_a_slice_maps_field_for_field() {
    assert_eq!(SubjectKindV1::WorkSlice.code(), "WORK_SLICE");
    assert!(SubjectKindV1::ALL.contains(&SubjectKindV1::WorkSlice));
    let s = slice();
    let part = s.challenge_subject();
    let subject = subject_of(&s);
    assert_eq!(subject.subject_id, digest(&s.slice_id()), "the subject is the slice identity");
    assert_eq!(part.verification_plan_root, s.plan_root);
    assert_eq!(part.input_root, s.input_root);
    assert_eq!(part.state_root, s.result_state_root, "the result boundary, not the predecessor");
    assert_eq!(part.constraint_root, s.evidence_root);
    assert_eq!(part.commitment_root, s.challenge_binding());
    assert_eq!(part.kernel_version, s.kernel_version);
    assert_eq!(subject_of(&s).id(), subject.id(), "deterministic");
}

#[test]
fn no_field_of_a_slice_escapes_the_contracts_subject() {
    let base = slice();
    let base_id = subject_of(&base).id();
    let flips: Vec<(&str, Box<dyn Fn(&mut PalwWorkSliceV1)>)> = vec![
        ("root_claim_id", Box::new(|s| s.root_claim_id = hash(900))),
        ("slice_index", Box::new(|s| s.slice_index += 1)),
        ("class_id", Box::new(|s| s.class_id = hash(901))),
        ("canonical_job_id", Box::new(|s| s.canonical_job_id = hash(902))),
        ("kernel_version", Box::new(|s| s.kernel_version += 1)),
        ("plan_root", Box::new(|s| s.plan_root = hash(903))),
        ("range.start", Box::new(|s| s.canonical_range.start += 1)),
        ("range.end", Box::new(|s| s.canonical_range.end += 1)),
        ("predecessor_state_root", Box::new(|s| s.predecessor_state_root = hash(904))),
        ("result_state_root", Box::new(|s| s.result_state_root = hash(905))),
        ("input_root", Box::new(|s| s.input_root = hash(906))),
        ("output_root", Box::new(|s| s.output_root = hash(907))),
        ("evidence_root", Box::new(|s| s.evidence_root = hash(908))),
        ("da_root", Box::new(|s| s.da_root = hash(909))),
        ("executor_bond", Box::new(|s| s.executor_bond = bond(8))),
    ];
    assert_eq!(flips.len(), 15, "fourteen fields, the range counted as its two ends");
    for (name, flip) in &flips {
        let mut changed = base.clone();
        flip(&mut changed);
        assert_ne!(subject_of(&changed).id(), base_id, "changing {name} changes the contract's subject");
    }
}

#[test]
fn the_pre_beacon_commitment_binds_what_the_typed_roots_do_not_carry() {
    let base = slice();
    let commitment = base.challenge_binding();
    let carried_only_by_the_commitment: Vec<(&str, Box<dyn Fn(&mut PalwWorkSliceV1)>)> = vec![
        ("root_claim_id", Box::new(|s| s.root_claim_id = hash(900))),
        ("slice_index", Box::new(|s| s.slice_index += 1)),
        ("range.start", Box::new(|s| s.canonical_range.start += 1)),
        ("range.end", Box::new(|s| s.canonical_range.end += 1)),
        ("predecessor_state_root", Box::new(|s| s.predecessor_state_root = hash(904))),
        ("output_root", Box::new(|s| s.output_root = hash(907))),
        ("da_root", Box::new(|s| s.da_root = hash(909))),
        ("canonical_job_id", Box::new(|s| s.canonical_job_id = hash(902))),
        ("executor_bond", Box::new(|s| s.executor_bond = bond(8))),
    ];
    for (name, flip) in &carried_only_by_the_commitment {
        let mut changed = base.clone();
        flip(&mut changed);
        assert_ne!(changed.challenge_binding(), commitment, "the commitment binds {name}");
    }
    // And the typed roots carry the rest: the commitment does not need to.
    let mut other_result = base.clone();
    other_result.result_state_root = hash(905 + 1);
    assert_eq!(other_result.challenge_binding(), commitment, "the result boundary is a typed root, not part of the binding");
    assert_ne!(other_result.challenge_subject().state_root, base.challenge_subject().state_root);
}

/// The contract tags the lane's slices `ExecWorkSlice` and its own test (`misaka-palw-challenge/tests/contract.rs`,
/// `only_fresh_final_da_satisfied_useful_work_of_active_g14_profiles_is_a_source`) pins `NotUsefulWork` for that kind: a slice is
/// never a beacon source. Only the kind's wire value is pinned here, since it is what a node tagging a slice would write.
#[test]
fn a_work_slice_has_the_contracts_own_source_kind() {
    assert_eq!(WorkSourceKindV1::ExecWorkSlice as u8, 4);
}
