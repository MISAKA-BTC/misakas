//! Preflight identity validation must not allocate before native reservation.
//! This allocator counter is thread-local and applies only to the measured
//! decode/hash interval; fixture construction and test-harness work are outside.
use misaka_palw_positive_contract::{identity::*, messages::*, native::*, rows::*};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}
struct Counting;
fn mark() {
    let enabled = TRACK.try_with(Cell::get).unwrap_or(false);
    if enabled {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
    }
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        mark();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        mark();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, n: usize) -> *mut u8 {
        mark();
        unsafe { System.realloc(p, layout, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn hash_reference<T: borsh::BorshSerialize>(domain: &[u8], value: &T) -> Digest {
    Digest(blake2b_simd::Params::new().hash_length(64).key(domain).hash(&borsh::to_vec(value).unwrap()).as_bytes().try_into().unwrap())
}
#[test]
fn fixed_native_result_decode_and_canonical_identity_hashes_allocate_zero_bytes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/contract-vectors-r1.json")).unwrap();
    let hex = fixture["result_borsh_hex"].as_str().unwrap();
    let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    let mut result: MinerResultCommit = borsh::from_slice(&bytes).unwrap();
    let key = [7; PUBKEY_BYTES];
    result.miner = principal(&key);
    let policy = d(9);
    let fired = d(8);
    result.scope.ruleset = ruleset_id(fired, policy);
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64([3; 64]),
        accepted_tx_ordinal: 4,
        event_ordinal: 5,
        accepted_daa: 6,
        accepted_blue_score: 7,
    };
    let row = NativeRowValue::new(NativeRowBody::Result(ResultRow {
        result: result.clone(),
        miner_key: key,
        miner_signature: [0; SIGNATURE_BYTES],
        origin,
        policy,
        fired_fork: fired,
        funding_scope: FundingScopeId(d(10)),
        dependencies: AcceptedMaterial { origin, digest: result.dependencies },
        role_assignment: AcceptedMaterial { origin, digest: d(11) },
    }))
    .unwrap();
    let row_key = row.body.key().encode();
    let encoded = row.encode().unwrap();
    let expected_id = ResultId(hash_reference(b"misaka/positive/result-id/v2/r1", &result));
    let expected_claim = ClaimId(hash_reference(b"misaka/positive/claim-id/v2/r1", &expected_id));
    let expected_case =
        CaseId(hash_reference(b"misaka/positive/case-id/v2/r1", &(result.scope, result.job, result.execution, result.miner)));
    // Initialize TLS and all fixture allocations before the measured interval.
    ALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|n| n.set(true));
    let decoded = NativeRowValue::decode(std::hint::black_box(&row_key), std::hint::black_box(&encoded));
    let id = result.id();
    let claim = result.claim_id();
    let case = result.case_id();
    let actual_ruleset = ruleset_id(fired, policy);
    TRACK.with(|n| n.set(false));
    let allocations = ALLOCATIONS.with(Cell::get);
    assert_eq!(allocations, 0, "fixed preflight must not request heap storage");
    assert_eq!(decoded.unwrap(), row);
    assert_eq!((id, claim, case), (expected_id, expected_claim, expected_case));
    assert_eq!(actual_ruleset, result.scope.ruleset);
}
