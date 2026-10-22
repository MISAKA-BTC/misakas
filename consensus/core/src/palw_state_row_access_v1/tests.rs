use super::*;
use crate::config::params::{DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params};
use crate::palw_state_auth_map_v1::{AuthStateV1, KeyBoundV1, verify_update_v1};
use std::cell::Cell;
use std::collections::BTreeSet;

fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

fn limits() -> AuthProofLimitsV1 {
    AuthProofLimitsV1 { max_table_nodes: 1 << 20, max_directory_nodes: 1 << 12, max_tables: 16 }
}

/// An honest supplier over the full node's authenticated state, which can go silent or start lying.
struct Supplier<'a> {
    state: &'a AuthStateV1,
    silent: Cell<bool>,
    other_branch: Option<&'a AuthStateV1>,
    calls: Cell<u64>,
}

impl PalwRowSupplierV1 for Supplier<'_> {
    fn fetch(&self, table: PalwStateTableV1, ranges: &[KeyRangeV1]) -> Option<AuthStateRowProofV1> {
        self.calls.set(self.calls.get() + 1);
        if self.silent.get() {
            return None;
        }
        Some(self.other_branch.unwrap_or(self.state).prove_rows(&table.spec(), ranges))
    }
}

fn supplier(state: &AuthStateV1) -> Supplier<'_> {
    Supplier { state, silent: Cell::new(false), other_branch: None, calls: Cell::new(0) }
}

/// The authenticated state a full node would hold for `source`'s tables (parent / anchor are test values).
fn auth_state_of<S: PalwRowSourceV1>(source: &S, anchor: Hash64) -> AuthStateV1 {
    let mut auth = AuthStateV1::new(source.pin().state_root, anchor);
    for table in PalwStateTableV1::ALL {
        for (k, v) in source.read_range(table, &KeyRangeV1::all(), usize::MAX).unwrap().rows {
            auth.write(&table.spec(), k, Some(v)).unwrap();
        }
    }
    auth
}

fn probes(rows: &[(Vec<u8>, Vec<u8>)], table: PalwStateTableV1) -> Vec<Vec<u8>> {
    let len = if table == PalwStateTableV1::Bonds { 68 } else { 64 };
    let mut out: Vec<Vec<u8>> = vec![vec![0; len], vec![0xff; len], vec![0x55; len]];
    for (k, _) in rows.iter().take(32) {
        let mut a = k.clone();
        *a.last_mut().unwrap() ^= 1;
        out.push(a);
        let mut b = k.clone();
        b[0] ^= 0x80;
        out.push(b);
    }
    if table == PalwStateTableV1::KernelRouteAux {
        out.extend([vec![32], vec![32, 0], vec![200, 1, 2, 3]]);
    }
    out
}

/// **The conformance check** every fixture runs: the full-state adapter answers exactly what the in-memory state holds; the proven
/// (pruned) backend built from it answers identically; the cache and the overlay change no answer; the state root is untouched.
pub(crate) fn palw_row_access_conformance_v1(state: &PalwChainStateV2) {
    let root_before = state.state_root();
    let full = PalwFullStateRowSourceV1::new(state);
    assert_eq!(full.pin().state_root, root_before);

    // 1. The adapter is the state: every row, in order, by the order-preserving codec.
    for table in PalwStateTableV1::ALL {
        let rows = full.table_rows(table);
        assert!(rows.windows(2).all(|w| w[0].0 < w[1].0), "{table:?}: the key codec preserves the collection's order");
        for (k, v) in &rows {
            assert_eq!(full.read_row(table, k), PalwRowReadV1::Present(v.clone()), "{table:?}");
        }
        let present: BTreeSet<_> = rows.iter().map(|(k, _)| k.clone()).collect();
        for p in probes(&rows, table) {
            if !present.contains(&p) {
                assert_eq!(full.read_row(table, &p), PalwRowReadV1::Absent, "{table:?} probe");
            }
        }
        let bad: &[u8] = if table == PalwStateTableV1::KernelRouteAux { &[] } else { &[1, 2, 3] };
        assert_eq!(full.read_row(table, bad), PalwRowReadV1::Unavailable(PalwRowUnavailableV1::MalformedKey));
        assert_eq!(
            full.read_range(table, &KeyRangeV1::all(), usize::MAX).unwrap(),
            PalwRowRangeV1 { rows: rows.clone(), more: false }
        );
        // Paging by limit reassembles the table.
        let mut paged = Vec::new();
        let mut start = KeyBoundV1::Unbounded;
        loop {
            let page = full.read_range(table, &KeyRangeV1 { start: start.clone(), end: KeyBoundV1::Unbounded }, 3).unwrap();
            if let Some((k, _)) = page.rows.last() {
                start = KeyBoundV1::Excluded(k.clone());
            }
            let more = page.more;
            paged.extend(page.rows);
            if !more {
                break;
            }
        }
        assert_eq!(paged, rows, "{table:?} paged");
    }
    // Typed reads are the state's own getters.
    for (key, bond) in state.bonds_iter() {
        assert_eq!(full.bond(key).unwrap().as_ref(), Some(bond));
        assert_eq!(palw_bond_of_row_key_v1(&palw_bond_row_key_v1(key)).as_ref(), Some(key));
    }
    for (id, class) in state.classes_iter() {
        assert_eq!(full.class(id).unwrap().map(|c| borsh::to_vec(&c).unwrap()), Some(borsh::to_vec(class).unwrap()));
    }
    for (id, claim) in state.claims_iter() {
        assert_eq!(full.claim(id).unwrap().map(|c| borsh::to_vec(&c).unwrap()), Some(borsh::to_vec(claim).unwrap()));
    }
    for (id, _) in state.consumed_offences_iter() {
        assert_eq!(full.offence_consumed(id), Ok(true));
    }
    assert_eq!(full.offence_consumed(&h(0xEE)), Ok(state.consumed_offence(&h(0xEE)).is_some()));
    // The v1 light-client opening of the bonds holds the same values in the same order (v1 keys are borsh, ours big-endian).
    let v1 = crate::palw_state_proof_v1::prove_bonds_v1(state);
    let v1_values: Vec<_> = v1.collection.rows.iter().map(|(_, v)| v.clone()).collect();
    let our_values: Vec<_> = full.table_rows(PalwStateTableV1::Bonds).into_iter().map(|(_, v)| v).collect();
    assert_eq!(v1_values, our_values);

    // 2. The proven backend answers identically, row by row, probe by probe, range by range.
    let auth = auth_state_of(&full, h(1));
    let sup = supplier(&auth);
    let proven = PalwProvenRowSourceV1::new(auth.pin(), &sup, limits());
    for table in PalwStateTableV1::ALL {
        let rows = full.table_rows(table);
        for (k, _) in rows.iter().take(64) {
            assert_eq!(proven.read_row(table, k), full.read_row(table, k), "{table:?}");
        }
        for p in probes(&rows, table).into_iter().chain([vec![], vec![7; 63], vec![7; 65]]) {
            assert_eq!(proven.read_row(table, &p), full.read_row(table, &p), "{table:?} probe");
        }
        let all = KeyRangeV1::all();
        assert_eq!(proven.read_range(table, &all, usize::MAX), full.read_range(table, &all, usize::MAX));
        if let (Some(first), Some(last)) = (rows.first(), rows.last()) {
            let mid = KeyRangeV1 { start: KeyBoundV1::Excluded(first.0.clone()), end: KeyBoundV1::Included(last.0.clone()) };
            assert_eq!(proven.read_range(table, &mid, 2), full.read_range(table, &mid, 2));
        }
    }
    for (key, _) in state.bonds_iter().take(16) {
        assert_eq!(proven.bond(key), full.bond(key));
    }

    // 3. A cache too small to hold the table answers the same; eviction only moves where answers come from.
    for budget in [0usize, 300, 4096, 1 << 22] {
        let cached = PalwCachedRowSourceV1::new(&full, budget);
        for _ in 0..2 {
            for table in PalwStateTableV1::ALL {
                for (k, v) in full.table_rows(table).iter().take(64) {
                    assert_eq!(cached.read_row(table, k), PalwRowReadV1::Present(v.clone()));
                }
            }
        }
        assert!(cached.stats().used_bytes <= budget);
    }

    // 4. The overlay: writes are read back, the change set is exactly the writes, and it proves old → new against the table map.
    let mut overlay = PalwRowOverlayV1::new(&full);
    let rows = full.table_rows(PalwStateTableV1::Claims);
    let new_key = vec![0xA5; 64];
    overlay.write_row(PalwStateTableV1::Claims, &new_key, Some(vec![1, 2, 3])).unwrap();
    if let Some((k, _)) = rows.first() {
        overlay.write_row(PalwStateTableV1::Claims, k, None).unwrap();
        assert_eq!(overlay.read_row(PalwStateTableV1::Claims, k), PalwRowReadV1::Absent);
    }
    assert_eq!(overlay.read_row(PalwStateTableV1::Claims, &new_key), PalwRowReadV1::Present(vec![1, 2, 3]));
    let changes = overlay.into_changes();
    assert_eq!(changes.pin, full.pin());
    let map = palw_auth_map_of_table_v1(&full, PalwStateTableV1::Claims).unwrap();
    let updates = changes.auth_updates(PalwStateTableV1::Claims);
    let keys: Vec<_> = updates.iter().map(|u| u.key.clone()).collect();
    let mut expected = map.clone();
    for u in &updates {
        expected.write(u.key.clone(), u.new.clone()).unwrap();
    }
    let spec = PalwStateTableV1::Claims.spec();
    assert_eq!(verify_update_v1(&spec, &map.root(), &map.prove_keys(&keys), &updates, 1 << 20), Ok(expected.root()));

    // Nothing above touched the state.
    assert_eq!(state.state_root(), root_before);
}

#[test]
fn the_genesis_state_conforms() {
    palw_row_access_conformance_v1(&PalwChainStateV2::genesis());
}

// ---------------------------------------------------------------------------------------------------------------------------
// "Unavailable" is not "absent"

fn small_auth() -> AuthStateV1 {
    let mut s = AuthStateV1::new(h(0), h(9));
    for i in 0..20u8 {
        s.write(&PalwStateTableV1::ConsumedOffences.spec(), vec![i; 64], Some(vec![i])).unwrap();
    }
    s
}

#[test]
fn a_silent_supplier_is_unavailable_never_absent_and_is_not_cached() {
    let auth = small_auth();
    let sup = supplier(&auth);
    let proven = PalwProvenRowSourceV1::new(auth.pin(), &sup, limits());
    let cached = PalwCachedRowSourceV1::new(&proven, 1 << 16);
    let present = Hash64::from_bytes([3; 64]);
    let absent = Hash64::from_bytes([200; 64]);

    sup.silent.set(true);
    assert_eq!(cached.offence_consumed(&present), Err(PalwRowUnavailableV1::SupplierUnavailable));
    assert_eq!(cached.offence_consumed(&absent), Err(PalwRowUnavailableV1::SupplierUnavailable));
    assert_eq!(
        cached.read_range(PalwStateTableV1::ConsumedOffences, &KeyRangeV1::all(), 10),
        Err(PalwRowUnavailableV1::SupplierUnavailable)
    );
    assert_eq!(cached.stats().entries, 0, "an unavailable read is never cached");
    // A write over an unavailable row is refused, not recorded with a guessed old value.
    let mut overlay = PalwRowOverlayV1::new(&cached);
    assert_eq!(
        overlay.write_row(PalwStateTableV1::ConsumedOffences, present.as_byte_slice(), None),
        Err(PalwRowUnavailableV1::SupplierUnavailable)
    );
    assert!(overlay.changes().writes.is_empty());

    sup.silent.set(false);
    assert_eq!(cached.offence_consumed(&present), Ok(true));
    assert_eq!(cached.offence_consumed(&absent), Ok(false));
    // Served from the cache now, even with the supplier gone again; and an eviction sends the next read back to the supplier.
    sup.silent.set(true);
    assert_eq!(cached.offence_consumed(&present), Ok(true));
    cached.clear();
    assert_eq!(cached.offence_consumed(&present), Err(PalwRowUnavailableV1::SupplierUnavailable));
}

#[test]
fn a_lying_supplier_is_rejected_never_believed() {
    let auth = small_auth();
    // Another branch from the same parent with the same rows plus one change.
    let (_, other) = auth.child(h(77), &[(PalwStateTableV1::ConsumedOffences.spec(), vec![3; 64], None)]).unwrap();
    let liar = Supplier { state: &auth, silent: Cell::new(false), other_branch: Some(&other), calls: Cell::new(0) };
    let proven = PalwProvenRowSourceV1::new(auth.pin(), &liar, limits());
    let k = Hash64::from_bytes([3; 64]);
    assert_eq!(proven.offence_consumed(&k), Err(PalwRowUnavailableV1::ProofRejected));

    // A supplier that answers a different question (a proof of another key).
    struct Wrong<'a>(&'a AuthStateV1);
    impl PalwRowSupplierV1 for Wrong<'_> {
        fn fetch(&self, table: PalwStateTableV1, _: &[KeyRangeV1]) -> Option<AuthStateRowProofV1> {
            Some(self.0.prove_rows(&table.spec(), &[KeyRangeV1::point(&[9; 64])]))
        }
    }
    let wrong = PalwProvenRowSourceV1::new(auth.pin(), Wrong(&auth), limits());
    assert_eq!(wrong.offence_consumed(&k), Err(PalwRowUnavailableV1::ProofRejected));
    assert_eq!(wrong.offence_consumed(&Hash64::from_bytes([9; 64])), Ok(true));
    // A table the directory proves absent answers every key absent — that is proven.
    let honest = supplier(&auth);
    let proven = PalwProvenRowSourceV1::new(auth.pin(), &honest, limits());
    assert_eq!(proven.claim(&k).unwrap().map(|_| ()), None);
}

#[test]
fn the_cache_never_serves_another_roots_answer() {
    let a = small_auth();
    let (_, b) = a.child(h(5), &[(PalwStateTableV1::ConsumedOffences.spec(), vec![3; 64], None)]).unwrap();
    let (sa, sb) = (supplier(&a), supplier(&b));
    let k = Hash64::from_bytes([3; 64]);
    // One cache shared through a source that switches pins.
    struct Switch<'a> {
        a: PalwProvenRowSourceV1<&'a Supplier<'a>>,
        b: PalwProvenRowSourceV1<&'a Supplier<'a>>,
        on_b: Cell<bool>,
    }
    impl PalwRowSourceV1 for Switch<'_> {
        fn pin(&self) -> PalwRowPinV1 {
            if self.on_b.get() { self.b.pin() } else { self.a.pin() }
        }
        fn read_row(&self, t: PalwStateTableV1, k: &[u8]) -> PalwRowReadV1 {
            if self.on_b.get() { self.b.read_row(t, k) } else { self.a.read_row(t, k) }
        }
        fn read_range(&self, t: PalwStateTableV1, r: &KeyRangeV1, l: usize) -> Result<PalwRowRangeV1, PalwRowUnavailableV1> {
            if self.on_b.get() { self.b.read_range(t, r, l) } else { self.a.read_range(t, r, l) }
        }
    }
    let sw = Switch {
        a: PalwProvenRowSourceV1::new(a.pin(), &sa, limits()),
        b: PalwProvenRowSourceV1::new(b.pin(), &sb, limits()),
        on_b: Cell::new(false),
    };
    let cached = PalwCachedRowSourceV1::new(&sw, 1 << 16);
    assert_eq!(cached.offence_consumed(&k), Ok(true));
    sw.on_b.set(true);
    assert_eq!(cached.offence_consumed(&k), Ok(false), "b deleted the row; a's cached answer is not b's");
    sw.on_b.set(false);
    assert_eq!(cached.offence_consumed(&k), Ok(true));
    assert_eq!(cached.stats().hits, 1);
}

#[test]
fn eviction_follows_lru_under_the_byte_budget() {
    let auth = small_auth();
    let sup = supplier(&auth);
    let proven = PalwProvenRowSourceV1::new(auth.pin(), &sup, limits());
    let per_entry = ENTRY_OVERHEAD + 64 + 1;
    let cached = PalwCachedRowSourceV1::new(&proven, per_entry * 4);
    for i in 0..10u8 {
        assert_eq!(cached.offence_consumed(&Hash64::from_bytes([i; 64])), Ok(true));
    }
    let s = cached.stats();
    assert_eq!((s.entries, s.evictions, s.misses), (4, 6, 10));
    assert!(s.used_bytes <= per_entry * 4);
    let calls = sup.calls.get();
    assert_eq!(cached.offence_consumed(&Hash64::from_bytes([9; 64])), Ok(true));
    assert_eq!(sup.calls.get(), calls, "the most recent entry is still cached");
    assert_eq!(cached.offence_consumed(&Hash64::from_bytes([0; 64])), Ok(true));
    assert_eq!(sup.calls.get(), calls + 1, "the oldest was evicted and is fetched again — with the same answer");
}

#[test]
fn the_overlay_merges_ranges_under_a_limit() {
    let auth = small_auth();
    let sup = supplier(&auth);
    let base = PalwProvenRowSourceV1::new(auth.pin(), &sup, limits());
    let t = PalwStateTableV1::ConsumedOffences;
    let mut overlay = PalwRowOverlayV1::new(&base);
    overlay.write_row(t, &[0; 64], None).unwrap();
    overlay.write_row(t, &[1; 64], None).unwrap();
    overlay.write_row(t, &[2; 64], Some(vec![42])).unwrap();
    overlay.write_row(t, &[250; 64], Some(vec![7])).unwrap();
    overlay.write_row(t, &[250; 64], Some(vec![8])).unwrap(); // a second write keeps the first old value
    let mut reference: BTreeMap<Vec<u8>, Vec<u8>> = (0..20u8).map(|i| (vec![i; 64], vec![i])).collect();
    reference.remove(&vec![0; 64]);
    reference.remove(&vec![1; 64]);
    reference.insert(vec![2; 64], vec![42]);
    reference.insert(vec![250; 64], vec![8]);
    let reference: Vec<_> = reference.into_iter().collect();
    for limit in [1usize, 2, 3, 5, 100] {
        let mut got = Vec::new();
        let mut start = KeyBoundV1::Unbounded;
        loop {
            let page = overlay.read_range(t, &KeyRangeV1 { start: start.clone(), end: KeyBoundV1::Unbounded }, limit).unwrap();
            assert!(page.rows.len() <= limit);
            if let Some((k, _)) = page.rows.last() {
                start = KeyBoundV1::Excluded(k.clone());
            }
            let more = page.more;
            got.extend(page.rows);
            if !more {
                break;
            }
        }
        assert_eq!(got, reference, "limit {limit}");
    }
    let changes = overlay.into_changes();
    assert_eq!(changes.writes.len(), 4);
    assert_eq!(changes.writes[&(t, vec![250; 64])], PalwRowWriteV1 { old: None, new: Some(vec![8]) });
    // The inverse undoes it.
    let inv = changes.inverse(PalwRowPinV1 { state_root: h(1) });
    assert_eq!(inv.writes[&(t, vec![0; 64])], PalwRowWriteV1 { old: None, new: Some(vec![0]) });
}

// ---------------------------------------------------------------------------------------------------------------------------
// The fence

#[test]
fn the_row_map_fence_is_dormant_everywhere_and_refused_if_armed() {
    for (name, p) in [
        ("mainnet", MAINNET_PARAMS),
        ("testnet", TESTNET_PARAMS),
        ("simnet", SIMNET_PARAMS),
        ("devnet", DEVNET_PARAMS),
        ("testnet-12 shipped", palw_t12_shipped_params()),
    ] {
        assert_eq!(p.palw_state_row_map_v1, None, "{name}");
        assert!(p.validate_palw_state_row_map_v1().is_ok(), "{name}");
        // Some-only: an armed height moves both ids, so the unarmed ids are a build's without the field.
        let mut armed = p.clone();
        armed.palw_state_row_map_v1 = Some(ForkActivation::new(1_000_000));
        assert_ne!(armed.consensus_params_id(), p.consensus_params_id(), "{name}");
        assert_ne!(armed.consensus_schedule_id(), p.consensus_schedule_id(), "{name}");
        assert!(armed.validate_palw_state_row_map_v1().is_err(), "{name}");
        // `never()` reads as unarmed.
        armed.palw_state_row_map_v1 = Some(ForkActivation::never());
        assert_eq!(armed.palw_state_row_map_v1_fence(), None);
    }
}
