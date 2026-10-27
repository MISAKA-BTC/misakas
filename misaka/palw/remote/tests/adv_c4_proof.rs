//! **Agent C4 — forging a PALW state proof** (RFC-0009 stage D: `getPalwStateProof`, header → state root → collection → row).
//!
//! The client believes a bond / class / claim only if a header it PINNED commits the state that holds it. Everything else on the wire
//! (the header bytes, the state-root preimage, the collection rows, the hex file form) is attacker-supplied. Expected results are
//! stated before each run; `#[ignore = "FAIL …"]` marks a proven defect (run with `-- --ignored`).

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_state_proof_v1::{
    PalwCollectionOpeningV1, PalwFactProofV1, PalwProofErrorV1, PalwStateOpeningV1, state_root_of_pinned_header_v1, verify_class_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBondKeyV2, PalwClassStateV2, PalwClassStatusV2, PalwPwuRuleV2, palw_collection_root_of_entries_v1,
    palw_state_root_of_preimage_v1,
};
use kaspa_hashes::Hash64;
use misaka_palw_remote::proof::{RegistrationAtPinV1, StateProofV1, registration_at_pin_v1};

fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

fn header_with(algo: u8, version: u16, root: Hash64) -> Header {
    let mut header = Header::new_finalized(
        version,
        vec![vec![h(1)]].try_into().unwrap(),
        h(2),
        h(3),
        h(4),
        1,
        0x1d00ffff,
        0,
        algo,
        55,
        0u64.into(),
        0,
        h(5),
    )
    .with_palw_state_root(root);
    header.finalize();
    header
}

fn class_record(root: Hash64, registrant: Option<PalwBondKeyV2>) -> PalwClassStateV2 {
    PalwClassStateV2 {
        artifact_root: root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(7),
        status: PalwClassStatusV2::Active,
        registered_daa: 10,
        registrant_bond: registrant,
        fused_attention: false,
    }
}

/// A hand-committed state whose class table holds `rows`; returns (state root, proof).
fn committed_classes(rows: Vec<(Hash64, PalwClassStateV2)>) -> (Hash64, PalwFactProofV1) {
    let rows: Vec<(Vec<u8>, Vec<u8>)> = rows.iter().map(|(k, r)| (borsh::to_vec(k).unwrap(), borsh::to_vec(r).unwrap())).collect();
    let root = palw_collection_root_of_entries_v1(b"classes", rows.len(), rows.iter().cloned());
    let mut preimage = vec![0xAA; 13];
    preimage.extend_from_slice(root.as_bytes().as_slice());
    preimage.extend_from_slice(&[0xBB; 9]);
    let state_root = palw_state_root_of_preimage_v1(&preimage);
    (
        state_root,
        PalwFactProofV1 {
            state: PalwStateOpeningV1 { preimage },
            collection: PalwCollectionOpeningV1 { label: b"classes".to_vec(), rows },
        },
    )
}

/// EXPECTED (before running): the pinned-header check recomputes the header hash from the header's CONTENT and refuses a header that
/// does not hash to the pinned block — in particular a header whose `palw_state_root` was edited without re-hashing. That holds
/// only if the header hash commits `palw_state_root`. The hash includes it only for V2-lineage algorithms and the heartbeat lane
/// (and only when non-zero); for every OTHER `pow_algo_id` the field is hash-invisible, so a hostile node can attach ANY state root to
/// a header that still hashes to the pinned block, and the proof "PROVEN against pinned block" is built over the attacker's state.
/// The safe property asserted: for EVERY algorithm id, a tampered root is refused. FAILS where the root is hash-invisible.
#[test]
#[ignore = "FAIL F-C4-10: a header whose algorithm does not hash palw_state_root lets a node attach any state root to a pinned block"]
fn a_state_root_edited_in_a_header_that_still_hashes_to_the_pin_must_be_refused_for_every_algorithm() {
    let (genuine, forged) = (h(0x70), h(0x71));
    let mut forgeable = Vec::new();
    for algo in 0..=255u8 {
        let real = header_with(algo, 1, genuine);
        let pinned = real.hash;
        let mut tampered = real.clone();
        tampered.palw_state_root = forged; // NOT re-finalized: `hash` is recomputed from content by the verifier
        if state_root_of_pinned_header_v1(&tampered, pinned) == Ok(forged) {
            forgeable.push(algo);
        }
    }
    // The version-0 genesis shape too (genesis is the likeliest "pinned" block of a fresh client).
    for version in [0u16, 1, 2] {
        let real = header_with(0, version, genuine);
        let mut tampered = real.clone();
        tampered.palw_state_root = forged;
        if state_root_of_pinned_header_v1(&tampered, real.hash) == Ok(forged) {
            forgeable.push(200 + version as u8);
        }
    }
    eprintln!("F-C4-10: algorithm ids whose pinned header accepts an edited palw_state_root: {forgeable:?}");
    assert!(forgeable.is_empty(), "the state root is hash-invisible for algorithms {forgeable:?}");
}

/// EXPECTED (before running): for the lineage that DOES hash the root (the V2 ids and the heartbeat lane) an edited root is refused as
/// not-the-pinned-block, a zero root is `HeaderCommitsNoState`, and any other edit of the header (daa score, parents, timestamp)
/// is refused. VERDICT: PASS.
#[test]
fn for_the_algorithms_that_hash_the_root_every_header_edit_is_the_wrong_block() {
    let hashed: Vec<u8> = (0..=255u8)
        .filter(|a| {
            kaspa_consensus_core::pow_layer0::is_palw_v2_algo_id(*a)
                || *a == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID
        })
        .collect();
    eprintln!("algorithms that hash palw_state_root: {hashed:?}");
    assert!(!hashed.is_empty());
    for algo in hashed {
        let real = header_with(algo, 1, h(0x70));
        let mut t = real.clone();
        t.palw_state_root = h(0x71);
        assert_eq!(state_root_of_pinned_header_v1(&t, real.hash), Err(PalwProofErrorV1::HeaderIsNotThePinnedBlock), "algo {algo}");
        let mut t = real.clone();
        t.daa_score += 1;
        assert_eq!(state_root_of_pinned_header_v1(&t, real.hash), Err(PalwProofErrorV1::HeaderIsNotThePinnedBlock));
        let mut t = real.clone();
        t.timestamp += 1;
        assert_eq!(state_root_of_pinned_header_v1(&t, real.hash), Err(PalwProofErrorV1::HeaderIsNotThePinnedBlock));
        let zero = header_with(algo, 1, Hash64::default());
        assert_eq!(state_root_of_pinned_header_v1(&zero, zero.hash), Err(PalwProofErrorV1::HeaderCommitsNoState), "algo {algo}");
        assert_eq!(state_root_of_pinned_header_v1(&real, real.hash), Ok(h(0x70)));
    }
}

/// EXPECTED (before running): a proof is a flat opening, so every way to lie about it changes a hash: the preimage (one byte), the
/// label, a row's value (swap the registrant for the attacker's), a row's key, a dropped / duplicated / reordered / extra row,
/// a truncated row, and rows of another collection under the "classes" label are each refused — and hiding the row can never turn
/// into `Absent`. VERDICT: PASS.
#[test]
fn every_edit_of_a_state_proof_opening_is_refused_and_hiding_a_row_never_proves_absence() {
    let (ours, thief) = (
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(7), 1)),
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(8), 0)),
    );
    let (class_a, class_b, class_c) = (h(0x10), h(0x11), h(0x12));
    let rows = vec![
        (class_a, class_record(h(0xA0), Some(ours))),
        (class_b, class_record(h(0xB0), Some(thief))),
        (class_c, class_record(h(0xC0), None)),
    ];
    let (state_root, proof) = committed_classes(rows.clone());
    assert_eq!(verify_class_v1(&proof, state_root, &class_a).unwrap().artifact_root, h(0xA0));
    assert_eq!(verify_class_v1(&proof, state_root, &h(0x99)), Err(PalwProofErrorV1::Absent), "a genuinely absent class");

    let mut edits: Vec<(&str, PalwFactProofV1)> = Vec::new();
    for i in [0usize, 5, 12, 13, 40, 70, 85] {
        let mut p = proof.clone();
        p.state.preimage[i] ^= 1;
        edits.push(("preimage byte", p));
    }
    let mut p = proof.clone();
    p.collection.label = b"bonds".to_vec();
    edits.push(("label", p));
    let mut p = proof.clone();
    p.collection.label = b"classe".to_vec();
    edits.push(("short label", p));
    for k in 0..proof.collection.rows.len().min(3) {
        let mut p = proof.clone();
        let last = p.collection.rows[k].1.len() - 1;
        p.collection.rows[k].1[last] ^= 1;
        edits.push(("row value", p));
        let mut p = proof.clone();
        p.collection.rows[k].0[0] ^= 1;
        edits.push(("row key", p));
        let mut p = proof.clone();
        p.collection.rows[k].1.pop();
        edits.push(("truncated row", p));
        let mut p = proof.clone();
        p.collection.rows.remove(k);
        edits.push(("dropped row", p));
        let mut p = proof.clone();
        let dup = p.collection.rows[k].clone();
        p.collection.rows.push(dup);
        edits.push(("duplicated row", p));
    }
    let mut p = proof.clone();
    p.collection.rows.swap(0, 2);
    edits.push(("reordered rows", p));
    let mut p = proof.clone();
    p.collection.rows.push((borsh::to_vec(&h(0x99)).unwrap(), borsh::to_vec(&class_record(h(0xEE), Some(thief))).unwrap()));
    edits.push(("extra attacker row", p));
    let mut p = proof.clone();
    p.collection.rows.clear();
    edits.push(("all rows hidden", p));
    for (what, e) in edits {
        for class in [class_a, class_b, class_c, h(0x99)] {
            match verify_class_v1(&e, state_root, &class) {
                Err(PalwProofErrorV1::Absent) => panic!("{what}: an edited opening proved ABSENCE of {class}"),
                Err(_) => {}
                Ok(_) => panic!("{what}: an edited opening verified class {class}"),
            }
        }
    }
}

/// EXPECTED (before running): `registration_at_pin_v1` is the client's belief about a registration; with proofs that are forged in
/// each of the ways above it returns an ERROR (never `Registered`, never `NotYetRegistered`); and the genuine proof says
/// Registered / Misattributed exactly. VERDICT: PASS.
#[test]
fn a_forged_proof_is_an_error_never_registered_or_not_yet_registered() {
    let ours = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(7), 1));
    let class = h(0x10);
    let (root, proof) = committed_classes(vec![(class, class_record(h(0xA0), Some(ours)))]);
    let header = header_with(6, 1, root);
    assert!(
        matches!(registration_at_pin_v1(&header, header.hash, &proof, &class, h(0xA0), &ours), Ok(RegistrationAtPinV1::Registered(p)) if p.is_proven())
    );
    // The proof of a DIFFERENT state under the same pin.
    let (_, other) = committed_classes(vec![(
        class,
        class_record(h(0xA0), Some(PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(9), 9)))),
    )]);
    assert!(
        registration_at_pin_v1(&header, header.hash, &other, &class, h(0xA0), &ours).is_err(),
        "another state's proof under our pin"
    );
    let mut hidden = proof.clone();
    hidden.collection.rows.clear();
    assert!(registration_at_pin_v1(&header, header.hash, &hidden, &class, h(0xA0), &ours).is_err());
    // A header that is not the pin.
    let other_header = header_with(6, 1, h(0x01));
    assert!(registration_at_pin_v1(&other_header, header.hash, &proof, &class, h(0xA0), &ours).is_err());
}

/// EXPECTED (before running): the hex file form of a proof (`StateProofV1`, embedded in bundles) never panics on any malformed
/// text — odd hex, non-hex, empty, truncated header bytes, giant counts — and what it opens is judged only by the verifiers above.
/// VERDICT: PASS.
#[test]
fn the_state_proof_file_form_never_panics_on_garbage() {
    let ours = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(7), 1));
    let (root, proof) = committed_classes(vec![(h(0x10), class_record(h(0xA0), Some(ours)))]);
    let header = header_with(6, 1, root);
    let good = StateProofV1::new(header.hash, &header, &proof);
    assert!(good.open().is_ok());
    let json = serde_json::to_string(&good).unwrap();
    let chars: Vec<char> = json.chars().collect();
    let mut opened = 0u32;
    for i in (0..chars.len()).step_by(7) {
        for rep in ['0', 'g', ' ', '"', 'f', '\u{00e9}'] {
            let mut m = chars.clone();
            m[i] = rep;
            let t: String = m.into_iter().collect();
            let r = std::panic::catch_unwind(|| serde_json::from_str::<StateProofV1>(&t).ok().map(|s| s.open().is_ok()));
            match r {
                Ok(Some(true)) => opened += 1,
                Ok(_) => {}
                Err(_) => panic!("the proof file panicked at char {i} -> {rep:?}"),
            }
        }
    }
    for t in ["", "{}", "null", "[]", "{\"block\":1}"] {
        assert!(std::panic::catch_unwind(|| serde_json::from_str::<StateProofV1>(t).ok().map(|s| s.open())).is_ok());
    }
    eprintln!("mutants that still open: {opened}");
    let mut odd = good.clone();
    odd.header.push('0');
    assert!(odd.open().is_err());
    let mut short = good.clone();
    short.header.truncate(40);
    assert!(short.open().is_err());
    let mut giant = good.clone();
    giant.rows = vec![("00".repeat(10), "ab".repeat(10)); 200_000];
    assert!(giant.open().is_ok(), "decoding proves nothing; it must not panic or blow up");
}
