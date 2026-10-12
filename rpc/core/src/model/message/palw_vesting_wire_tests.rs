//! **`message`'s `palw_vesting_wire_tests` module**, moved out of `message.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;

fn v3_row(id: &str) -> RpcPalwClaimRow {
    RpcPalwClaimRow {
        claim_id: id.to_string(),
        class_id: "c1".to_string(),
        phase: "final".to_string(),
        escrow_sompi: 1_000,
        exec_stage: "credited".to_string(),
        exec_credit: 7,
        vesting_stage: "latched".to_string(),
        vesting_sompi: 1_000,
        vesting_payee_sompi: 500,
        vesting_expiry_daa: Some(9_000),
        vesting_licences_since_final: 12,
        vesting_licences_needed: Some(30),
        vesting_matured_at: Some(9_100),
        vesting_eta_daa: Some(9_101),
        vesting_eta_estimated: false,
        ..Default::default()
    }
}

/// The version-2 reader exactly as it shipped: the version-1 fields, then the version-2 ones,
/// and nothing after.
struct V2Reader(RpcPalwClaimRow);

impl Deserializer for V2Reader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let mut row = RpcPalwClaimRow {
            claim_id: load!(String, reader)?,
            is_free_prompt: load!(bool, reader)?,
            class_id: load!(String, reader)?,
            executor_bond: load!(String, reader)?,
            phase: load!(String, reader)?,
            void_reason: load!(String, reader)?,
            phase_daa: load!(u64, reader)?,
            accepted_daa: load!(u64, reader)?,
            accepted_block: load!(String, reader)?,
            rebound_daa: load!(Option<u64>, reader)?,
            bound_daa: load!(Option<u64>, reader)?,
            seats: load!(Vec<String>, reader)?,
            deadline_daa: load!(Option<u64>, reader)?,
            reserved_sompi: load!(String, reader)?,
            escrow_sompi: load!(u64, reader)?,
            payout_pending_sompi: load!(Option<u64>, reader)?,
            quanta: load!(u32, reader)?,
            quanta_spent: load!(u32, reader)?,
            work_leaves: load!(u64, reader)?,
            open_courts: load!(u32, reader)?,
            ..Default::default()
        };
        if version >= 2 {
            row.exec_stage = load!(String, reader)?;
            row.exec_credit = load!(u64, reader)?;
            row.exec_span = load!(Option<u64>, reader)?;
            row.exec_tickets = load!(u32, reader)?;
            row.exec_tickets_spent = load!(u32, reader)?;
            row.exec_first_round = load!(Option<u64>, reader)?;
            row.exec_last_round = load!(Option<u64>, reader)?;
        }
        Ok(Self(row))
    }
}

/// **T51: a v2 client decodes a v3 row** — each row of the list is its own length-prefixed
/// payload, so the old reader stops before the vesting fields and the NEXT row still lands.
#[test]
fn t51_a_version_2_client_decodes_version_3_rows() {
    let rows = vec![v3_row("a"), v3_row("b")];
    let mut bytes = Vec::new();
    serialize!(Vec<RpcPalwClaimRow>, &rows, &mut bytes).unwrap();
    let old = deserialize!(Vec<V2Reader>, &mut bytes.as_slice()).unwrap();
    assert_eq!(old.iter().map(|r| r.0.claim_id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert_eq!((old[1].0.escrow_sompi, old[1].0.exec_credit), (1_000, 7), "the fields it knows are intact");
    assert!(old[1].0.vesting_stage.is_empty(), "and it never saw the new ones");
    // The v3 reader reads them all.
    let new = deserialize!(Vec<RpcPalwClaimRow>, &mut bytes.as_slice()).unwrap();
    assert_eq!(new, rows);
}

/// A version-2 peer's row reads with no vesting stage; its version-2 fields stand.
#[test]
fn a_version_2_row_reads_as_not_vesting() {
    let row = RpcPalwClaimRow { exec_credit: 9, ..Default::default() };
    let mut bytes = Vec::new();
    Serializer::serialize(&row, &mut bytes).unwrap();
    // Version 3 appended: "" (4) + u64 + u64 + None + u64 + None + None + None + bool; version 4
    // after it: "" (4).
    let tail = 4 + 8 + 8 + 1 + 8 + 1 + 1 + 1 + 1 + 4;
    bytes.truncate(bytes.len() - tail);
    bytes[..2].copy_from_slice(&2u16.to_le_bytes());
    let back = <RpcPalwClaimRow as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, row);
}

/// `getPalwClaims` v2 carries the retired claims' vesting rows; a version-1 answer (no tail)
/// reads with none, rather than failing at the end of its frame.
#[test]
fn the_claims_answer_gates_its_vesting_tail_on_its_version() {
    let response = GetPalwClaimsResponse {
        available: true,
        bond: "b:0".to_string(),
        claims: vec![v3_row("live")],
        bond_capable_classes: vec!["c1".to_string()],
        vesting_only_rows: vec![v3_row("retired")],
        vesting_only_truncated: true,
        ..Default::default()
    };
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwClaimsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.vesting_only_rows, response.vesting_only_rows);
    assert!(back.vesting_only_truncated);
    // A version-1 peer: the same bytes without the version-2 and version-3 tails.
    let mut tail = Vec::new();
    serialize!(Vec<RpcPalwClaimRow>, &response.vesting_only_rows, &mut tail).unwrap();
    bytes.truncate(bytes.len() - lock_tail_len(&response) - tail.len() - 1);
    bytes[..2].copy_from_slice(&1u16.to_le_bytes());
    let old = <GetPalwClaimsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert!(old.vesting_only_rows.is_empty() && !old.vesting_only_truncated);
    assert_eq!(old.claims, response.claims);
    assert_eq!(old.bond_capable_classes, response.bond_capable_classes);
}

/// The version-3 tail's length, as `GetPalwClaimsResponse`'s serializer writes it.
fn lock_tail_len(r: &GetPalwClaimsResponse) -> usize {
    let mut v3 = Vec::new();
    store!(u64, &r.bond_locks_at_daa, &mut v3).unwrap();
    store!(u64, &r.bond_live_lock_count, &mut v3).unwrap();
    store!(String, &r.bond_live_locked_sompi, &mut v3).unwrap();
    serialize!(Vec<RpcPalwBondLock>, &r.bond_locks, &mut v3).unwrap();
    store!(bool, &r.bond_locks_truncated, &mut v3).unwrap();
    store!(u64, &r.bond_lock_daa_clock_release, &mut v3).unwrap();
    store!(Option<u64>, &r.bond_lock_anchor_count_bound, &mut v3).unwrap();
    store!(bool, &r.bond_retire_refused_while_locked, &mut v3).unwrap();
    store!(u64, &r.bond_withdrawal_delay_daa, &mut v3).unwrap();
    v3.len()
}

/// **`getPalwClaims` v3 carries the bond's live locks** (testnet-12 lifecycle audit T12-058), and
/// a version-2 answer (no lock tail) reads with none — its vesting rows intact.
#[test]
fn the_claims_answer_gates_its_lock_tail_on_its_version() {
    let response = GetPalwClaimsResponse {
        available: true,
        bond: "b:0".to_string(),
        claims: vec![v3_row("live")],
        vesting_only_rows: vec![v3_row("retired")],
        bond_locks_at_daa: 357,
        bond_live_lock_count: 51,
        bond_live_locked_sompi: "1160619281472".to_string(),
        bond_locks: vec![RpcPalwBondLock {
            claim_id: "c".repeat(128),
            amount_sompi: "64017000000".to_string(),
            expiry_daa: 3_353,
            held_by_anchor_count: false,
        }],
        bond_locks_truncated: true,
        bond_lock_daa_clock_release: 3_353,
        bond_lock_anchor_count_bound: Some(9_353),
        bond_retire_refused_while_locked: true,
        bond_withdrawal_delay_daa: 12_900,
        ..Default::default()
    };
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwClaimsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.bond_locks.len(), 1);
    assert_eq!(back.bond_locks[0].expiry_daa, 3_353);
    assert_eq!(back.bond_live_locked_sompi, response.bond_live_locked_sompi);
    assert_eq!((back.bond_lock_anchor_count_bound, back.bond_withdrawal_delay_daa), (Some(9_353), 12_900));
    assert!(back.bond_retire_refused_while_locked && back.bond_locks_truncated);
    // A version-2 peer: the same bytes without the lock tail.
    bytes.truncate(bytes.len() - lock_tail_len(&response));
    bytes[..2].copy_from_slice(&2u16.to_le_bytes());
    let old = <GetPalwClaimsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(old.vesting_only_rows, response.vesting_only_rows, "version 2's tail is intact");
    assert!(old.bond_locks.is_empty() && old.bond_live_lock_count == 0 && !old.bond_retire_refused_while_locked);
}
