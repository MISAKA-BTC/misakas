use super::*;
use std::collections::BTreeMap;

const SPEC: AuthTableSpecV1 = AuthTableSpecV1::new(7, 1, 80, 256);
const BIG: u32 = 1 << 20;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
    /// `min + below(spread)` random bytes.
    fn bytes_len(&mut self, min: usize, spread: u64) -> Vec<u8> {
        let len = min + self.below(spread) as usize;
        self.bytes(len)
    }
    /// Short keys over a tiny alphabet: many shared prefixes, many keys that are prefixes of others.
    fn tight_key(&mut self) -> Vec<u8> {
        let len = self.below(5) as usize;
        (0..len).map(|_| self.below(3) as u8).collect()
    }
}

fn h(n: u8) -> crate::Hash64 {
    crate::Hash64::from_bytes([n; 64])
}

fn build(rows: &BTreeMap<Vec<u8>, Vec<u8>>) -> AuthMapV1 {
    let mut m = AuthMapV1::new(SPEC).unwrap();
    for (k, v) in rows {
        m.insert(k.clone(), v.clone()).unwrap();
    }
    m
}

fn random_rows(rng: &mut Rng, n: usize, tight: bool) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut rows = BTreeMap::new();
    while rows.len() < n {
        let k = if tight { rng.tight_key() } else { rng.bytes_len(1, 40) };
        let v = rng.bytes_len(0, 20);
        rows.insert(k, v);
    }
    rows
}

fn reference_range(rows: &BTreeMap<Vec<u8>, Vec<u8>>, r: &KeyRangeV1) -> Vec<(Vec<u8>, Vec<u8>)> {
    rows.iter().filter(|(k, _)| r.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn random_bound(rng: &mut Rng, tight: bool) -> KeyBoundV1 {
    let k = if tight { rng.tight_key() } else { rng.bytes_len(0, 6) };
    match rng.below(3) {
        0 => KeyBoundV1::Unbounded,
        1 => KeyBoundV1::Included(k),
        _ => KeyBoundV1::Excluded(k),
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// The encoding

#[test]
fn the_key_encoding_preserves_order_and_is_prefix_free() {
    let mut rng = Rng(1);
    let mut keys: Vec<Vec<u8>> = (0..400).map(|_| rng.tight_key()).collect();
    keys.extend((0..100).map(|_| rng.bytes_len(0, 4)));
    for a in &keys {
        for b in &keys {
            let (ea, eb) = (BitPathV1::encode_key(a), BitPathV1::encode_key(b));
            let bitwise = {
                let c = ea.common_prefix_len(&eb);
                if c == ea.len() && c == eb.len() {
                    std::cmp::Ordering::Equal
                } else if c == ea.len() {
                    std::cmp::Ordering::Less
                } else if c == eb.len() {
                    std::cmp::Ordering::Greater
                } else {
                    ea.bit(c).cmp(&eb.bit(c))
                }
            };
            assert_eq!(bitwise, a.cmp(b), "{a:?} vs {b:?}");
            if a != b {
                assert!(!ea.starts_with(&eb) && !eb.starts_with(&ea), "prefix-free: {a:?} {b:?}");
            }
        }
    }
    // A packed prefix with dirty padding is not a canonical spelling.
    assert!(BitPathV1::from_parts(vec![0b1010_0001], 4).is_none());
    assert!(BitPathV1::from_parts(vec![0b1010_0000], 4).is_some());
    assert!(BitPathV1::from_parts(vec![0, 0], 4).is_none());
}

#[test]
fn subtree_test_never_clears_a_subtree_that_holds_a_key_of_the_range() {
    // Exhaustive over a small key universe: for every prefix of every encoded key and every range, if some key extending the
    // prefix lies in the range, the test must say "may intersect".
    let mut universe: Vec<Vec<u8>> = vec![vec![]];
    for a in 0..3u8 {
        universe.push(vec![a]);
        for b in 0..3u8 {
            universe.push(vec![a, b]);
            for c in 0..3u8 {
                universe.push(vec![a, b, c]);
            }
        }
    }
    let mut bounds = vec![KeyBoundV1::Unbounded];
    for k in &universe {
        bounds.push(KeyBoundV1::Included(k.clone()));
        bounds.push(KeyBoundV1::Excluded(k.clone()));
    }
    let encoded: Vec<_> = universe.iter().map(|k| (k, BitPathV1::encode_key(k))).collect();
    let mut prefixes = std::collections::BTreeSet::new();
    for (_, e) in &encoded {
        for l in 0..=e.len() {
            prefixes.insert((e.prefix(l).packed().to_vec(), l));
        }
    }
    for s in &bounds {
        for e in &bounds {
            let range = KeyRangeV1 { start: s.clone(), end: e.clone() };
            for (packed, l) in &prefixes {
                let q = BitPathV1::from_parts(packed.clone(), *l).unwrap();
                let holds = encoded.iter().any(|(k, ek)| ek.starts_with(&q) && range.contains(k));
                if holds {
                    assert!(subtree_may_intersect_v1(&q, &range), "{range:?} prefix {l} bits");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// The map

#[test]
fn the_root_is_a_function_of_the_rows_whatever_the_write_order() {
    let mut rng = Rng(2);
    for tight in [false, true] {
        let rows = random_rows(&mut rng, if tight { 60 } else { 300 }, tight);
        let reference = build(&rows);
        let mut shuffled: Vec<_> = rows.iter().collect();
        for i in (1..shuffled.len()).rev() {
            shuffled.swap(i, rng.below(i as u64 + 1) as usize);
        }
        let mut m = AuthMapV1::new(SPEC).unwrap();
        // Write extra rows, then remove them: history must not show in the root.
        let extra = random_rows(&mut rng, 40, tight);
        for (k, v) in &extra {
            m.insert(k.clone(), v.clone()).unwrap();
        }
        for (k, v) in shuffled {
            m.insert(k.clone(), v.clone()).unwrap();
        }
        for k in extra.keys() {
            match rows.get(k) {
                Some(v) => assert!(m.insert(k.clone(), v.clone()).unwrap().is_some()),
                None => assert!(m.remove(k).unwrap().is_some()),
            }
        }
        assert_eq!(m.root(), reference.root());
        assert_eq!(m.len(), rows.len() as u64);
        assert_eq!(m.rows(), rows.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>());
        // Removing every row returns to the empty root.
        for k in rows.keys() {
            m.remove(k).unwrap();
        }
        assert_eq!(m.root(), AuthMapV1::new(SPEC).unwrap().root());
    }
}

#[test]
fn the_map_agrees_with_a_btreemap_under_random_operations() {
    let mut rng = Rng(3);
    let mut m = AuthMapV1::new(SPEC).unwrap();
    let mut r = BTreeMap::new();
    for step in 0..3000 {
        let k = rng.tight_key();
        if rng.below(3) == 0 {
            assert_eq!(m.remove(&k).unwrap(), r.remove(&k));
        } else {
            let v = rng.bytes(3);
            assert_eq!(m.insert(k.clone(), v.clone()).unwrap(), r.insert(k, v));
        }
        if step % 97 == 0 {
            assert_eq!(build(&r).root(), m.root(), "step {step}");
        }
        let probe = rng.tight_key();
        assert_eq!(m.get(&probe), r.get(&probe).map(Vec::as_slice));
    }
    assert_eq!(m.len(), r.len() as u64);
}

#[test]
fn limits_are_enforced_on_write() {
    let mut m = AuthMapV1::new(SPEC).unwrap();
    assert_eq!(m.insert(vec![0; 81], vec![]), Err(AuthMapErrorV1::KeyTooLong));
    assert_eq!(m.insert(vec![0; 80], vec![0; 257]), Err(AuthMapErrorV1::ValueTooLong));
    assert!(AuthMapV1::new(AuthTableSpecV1::new(1, 1, AUTH_MAP_MAX_KEY_LEN_V1 + 1, 1)).is_err());
}

// ---------------------------------------------------------------------------------------------------------------------------
// Membership, non-membership, ranges

#[test]
fn every_row_is_proven_present_and_every_probe_absent_or_present_exactly() {
    let mut rng = Rng(4);
    for tight in [false, true] {
        let rows = random_rows(&mut rng, if tight { 50 } else { 400 }, tight);
        let m = build(&rows);
        let root = m.root();
        for (k, v) in &rows {
            let proof = m.prove_keys(&[k]);
            assert!(proof.nodes.len() as u32 <= SPEC.point_proof_node_bound());
            let ok = verify_rows_v1(&SPEC, &root, &proof, &[KeyRangeV1::point(k)], SPEC.point_proof_node_bound()).unwrap();
            assert_eq!(ok.get(k).unwrap(), Some(v.as_slice()));
        }
        for _ in 0..300 {
            let k = if tight { rng.tight_key() } else { rng.bytes_len(0, 41) };
            let proof = m.prove_keys(&[&k]);
            let ok = verify_rows_v1(&SPEC, &root, &proof, &[KeyRangeV1::point(&k)], SPEC.point_proof_node_bound()).unwrap();
            assert_eq!(ok.get(&k).unwrap(), rows.get(&k).map(Vec::as_slice));
            // A key the proof was not asked about has no answer — not "absent".
            let other = [k.clone(), vec![9]].concat();
            if !KeyRangeV1::point(&k).contains(&other) {
                assert_eq!(ok.get(&other), Err(AuthMapErrorV1::NotCovered));
            }
        }
    }
    // The empty table proves every key absent with an empty proof.
    let empty = AuthMapV1::new(SPEC).unwrap();
    let ok = verify_rows_v1(&SPEC, &empty.root(), &empty.prove_keys(&[b"x"]), &[KeyRangeV1::point(b"x")], 1).unwrap();
    assert_eq!(ok.get(b"x").unwrap(), None);
}

#[test]
fn every_range_is_proven_complete() {
    let mut rng = Rng(5);
    for tight in [false, true] {
        let rows = random_rows(&mut rng, if tight { 60 } else { 300 }, tight);
        let m = build(&rows);
        for _ in 0..300 {
            let range = KeyRangeV1 { start: random_bound(&mut rng, tight), end: random_bound(&mut rng, tight) };
            let proof = m.prove(std::slice::from_ref(&range));
            let ok = verify_rows_v1(&SPEC, &m.root(), &proof, std::slice::from_ref(&range), BIG).unwrap();
            let got: Vec<_> = ok.range(&range).unwrap().into_iter().map(|(k, v)| (k.to_vec(), v.to_vec())).collect();
            assert_eq!(got, reference_range(&rows, &range), "{range:?}");
            assert_eq!(m.range_rows(&range, usize::MAX), got);
        }
        // With a raw prefix too.
        let p = KeyRangeV1::with_prefix(&[1]);
        let ok = verify_rows_v1(&SPEC, &m.root(), &m.prove(std::slice::from_ref(&p)), std::slice::from_ref(&p), BIG).unwrap();
        assert!(ok.range(&p).unwrap().iter().all(|(k, _)| k.first() == Some(&1)));
        assert_eq!(ok.range(&p).unwrap().len(), rows.keys().filter(|k| k.first() == Some(&1)).count());
    }
}

#[test]
fn proof_size_is_bounded_and_logarithmic_for_hash_keys() {
    let mut rng = Rng(6);
    let spec = AuthTableSpecV1::new(1, 1, 64, 64);
    let mut m = AuthMapV1::new(spec).unwrap();
    for _ in 0..4096 {
        m.insert(rng.bytes(64), rng.bytes(8)).unwrap();
    }
    let mut worst = 0;
    for _ in 0..200 {
        let k = rng.bytes(64);
        worst = worst.max(m.prove_keys(&[&k]).nodes.len());
    }
    // 4096 uniform keys: depth ≈ 12, a point proof ≈ 2·12+1 nodes; far under the hard bound of 2·(9·64+1)+1.
    assert!(worst <= 2 * 24 + 1, "worst point proof {worst} nodes");
    assert!(worst as u32 <= spec.point_proof_node_bound());
    // A ground chain (each key the previous plus one byte) is as deep as it gets, and still inside the bound.
    let mut deep = AuthMapV1::new(spec).unwrap();
    for len in 0..=64 {
        deep.insert(vec![0u8; len], vec![]).unwrap();
    }
    let p = deep.prove_keys(&[vec![0u8; 64]]);
    assert!(p.nodes.len() as u32 <= spec.point_proof_node_bound());
    assert!(verify_rows_v1(&spec, &deep.root(), &p, &[KeyRangeV1::point(&[0u8; 64])], spec.point_proof_node_bound()).is_ok());
}

// ---------------------------------------------------------------------------------------------------------------------------
// Adversarial proofs: every one must be rejected

fn fixture() -> (BTreeMap<Vec<u8>, Vec<u8>>, AuthMapV1) {
    let mut rng = Rng(7);
    let rows = random_rows(&mut rng, 120, false);
    let m = build(&rows);
    (rows, m)
}

#[test]
fn a_missing_row_is_rejected() {
    let (rows, m) = fixture();
    let keys: Vec<_> = rows.keys().cloned().collect();
    let range = KeyRangeV1 { start: KeyBoundV1::Included(keys[10].clone()), end: KeyBoundV1::Excluded(keys[30].clone()) };
    let honest = m.prove(std::slice::from_ref(&range));
    // (a) a leaf of the range replaced by its own (correct) hash: the root still matches, completeness does not.
    let leaf_at = honest
        .nodes
        .iter()
        .position(|n| matches!(n, ProofNodeV1::Leaf { key, .. } if range.contains(key)))
        .expect("the range has rows");
    let mut hidden = honest.clone();
    if let ProofNodeV1::Leaf { key, value } = &honest.nodes[leaf_at] {
        hidden.nodes[leaf_at] = ProofNodeV1::Opaque(trie::leaf_hash(&SPEC, key, value));
    }
    assert_eq!(
        verify_rows_v1(&SPEC, &m.root(), &hidden, std::slice::from_ref(&range), BIG).err(),
        Some(AuthMapErrorV1::IncompleteRange)
    );
    // (b) a leaf simply dropped: the preorder no longer parses to the root.
    let mut dropped = honest.clone();
    dropped.nodes.remove(leaf_at);
    assert!(verify_rows_v1(&SPEC, &m.root(), &dropped, std::slice::from_ref(&range), BIG).is_err());
    // (c) a whole opened branch of the range collapsed to its hash.
    let a = m.prove(&[KeyRangeV1::point(&keys[0])]);
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &a, std::slice::from_ref(&range), BIG).err(), Some(AuthMapErrorV1::IncompleteRange));
}

#[test]
fn a_false_non_existence_is_rejected() {
    let (rows, m) = fixture();
    let k = rows.keys().nth(40).unwrap();
    // (a) the proof of a neighbour does not answer for k.
    let neighbour = rows.keys().nth(41).unwrap();
    let p = m.prove_keys(&[neighbour]);
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &p, &[KeyRangeV1::point(k)], BIG).err(), Some(AuthMapErrorV1::IncompleteRange));
    let ok = verify_rows_v1(&SPEC, &m.root(), &p, &[KeyRangeV1::point(neighbour)], BIG).unwrap();
    assert_eq!(ok.get(k), Err(AuthMapErrorV1::NotCovered));
    // (b) k's leaf rewritten under another key (so k "is not there"): the hash moves.
    let mut renamed = m.prove_keys(&[k]);
    for n in renamed.nodes.iter_mut() {
        if let ProofNodeV1::Leaf { key, .. } = n
            && key == k
        {
            let mut other = k.clone();
            other.push(0);
            *key = other;
        }
    }
    assert!(verify_rows_v1(&SPEC, &m.root(), &renamed, &[KeyRangeV1::point(k)], BIG).is_err());
    // (c) the proof of a map WITHOUT k, against the root of the map with it.
    let mut without = rows.clone();
    without.remove(k);
    let w = build(&without);
    assert_eq!(
        verify_rows_v1(&SPEC, &m.root(), &w.prove_keys(&[k]), &[KeyRangeV1::point(k)], BIG).err(),
        Some(AuthMapErrorV1::RootMismatch)
    );
    // (d) a tampered value.
    let mut tampered = m.prove_keys(&[k]);
    for n in tampered.nodes.iter_mut() {
        if let ProofNodeV1::Leaf { value, .. } = n {
            value.push(1);
        }
    }
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &tampered, &[KeyRangeV1::point(k)], BIG).err(), Some(AuthMapErrorV1::RootMismatch));
    // (e) the empty proof (count 0) against a non-empty root.
    assert_eq!(
        verify_rows_v1(&SPEC, &m.root(), &AuthMapProofV1::default(), &[KeyRangeV1::point(k)], BIG).err(),
        Some(AuthMapErrorV1::RootMismatch)
    );
}

#[test]
fn an_omitted_range_is_rejected() {
    let (rows, m) = fixture();
    let keys: Vec<_> = rows.keys().cloned().collect();
    let short = KeyRangeV1 { start: KeyBoundV1::Included(keys[5].clone()), end: KeyBoundV1::Excluded(keys[20].clone()) };
    let long = KeyRangeV1 { start: KeyBoundV1::Included(keys[5].clone()), end: KeyBoundV1::Excluded(keys[60].clone()) };
    let p = m.prove(std::slice::from_ref(&short));
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &p, std::slice::from_ref(&long), BIG).err(), Some(AuthMapErrorV1::IncompleteRange));
    // Asking a verified proof for a range it was not verified for has no answer.
    let ok = verify_rows_v1(&SPEC, &m.root(), &p, std::slice::from_ref(&short), BIG).unwrap();
    assert_eq!(ok.range(&long).err(), Some(AuthMapErrorV1::NotCovered));
    // Two ranges asked, one proven (unless the far row happens to be opened as a boundary leaf, then it is answered truly).
    let other = KeyRangeV1::point(&keys[90]);
    let opened = p.nodes.iter().any(|n| matches!(n, ProofNodeV1::Leaf { key, .. } if *key == keys[90]));
    assert!(!opened, "the fixture's far row is not on the boundary");
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &p, &[short.clone(), other], BIG).err(), Some(AuthMapErrorV1::IncompleteRange));
}

#[test]
fn a_misplaced_or_malformed_proof_is_rejected() {
    let (rows, m) = fixture();
    let k = rows.keys().nth(3).unwrap().clone();
    let honest = m.prove_keys(&[&k]);
    // Children swapped under the root branch: each opened node is checked against the side it hangs on.
    let mut swapped = honest.clone();
    if let ProofNodeV1::Branch { .. } = swapped.nodes[0] {
        // find the split point of the root's two subtrees by re-parsing sizes
        fn size(nodes: &[ProofNodeV1], at: usize) -> usize {
            match &nodes[at] {
                ProofNodeV1::Branch { .. } => {
                    let l = size(nodes, at + 1);
                    1 + l + size(nodes, at + 1 + l)
                }
                _ => 1,
            }
        }
        let l = size(&swapped.nodes, 1);
        let left: Vec<_> = swapped.nodes[1..1 + l].to_vec();
        let right: Vec<_> = swapped.nodes[1 + l..].to_vec();
        swapped.nodes = std::iter::once(swapped.nodes[0].clone()).chain(right).chain(left).collect();
    }
    assert!(verify_rows_v1(&SPEC, &m.root(), &swapped, &[KeyRangeV1::point(&k)], BIG).is_err());
    // A trailing node.
    let mut trailing = honest.clone();
    trailing.nodes.push(ProofNodeV1::Opaque(h(1)));
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &trailing, &[KeyRangeV1::point(&k)], BIG).err(), Some(AuthMapErrorV1::Malformed));
    // A truncated proof.
    let mut truncated = honest.clone();
    truncated.nodes.pop();
    assert!(verify_rows_v1(&SPEC, &m.root(), &truncated, &[KeyRangeV1::point(&k)], BIG).is_err());
    // A branch prefix with dirty padding, or deeper than any key.
    let mut dirty = honest.clone();
    if let ProofNodeV1::Branch { prefix_len, prefix } = &mut dirty.nodes[0] {
        *prefix_len = 1;
        *prefix = vec![0b0100_0000];
    }
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &dirty, &[KeyRangeV1::point(&k)], BIG).err(), Some(AuthMapErrorV1::Malformed));
    let mut deep = honest.clone();
    deep.nodes[0] = ProofNodeV1::Branch { prefix_len: SPEC.max_depth(), prefix: vec![0; SPEC.max_depth().div_ceil(8) as usize] };
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &deep, &[KeyRangeV1::point(&k)], BIG).err(), Some(AuthMapErrorV1::Malformed));
    // Over the node bound, before any hashing; over the byte bound, before decoding.
    assert_eq!(
        verify_rows_v1(&SPEC, &m.root(), &honest, &[KeyRangeV1::point(&k)], honest.nodes.len() as u32 - 1).err(),
        Some(AuthMapErrorV1::ProofTooLarge)
    );
    let bytes = honest.to_bytes();
    assert_eq!(AuthMapProofV1::from_bytes(&bytes, bytes.len()).unwrap(), honest);
    assert_eq!(AuthMapProofV1::from_bytes(&bytes, bytes.len() - 1).err(), Some(AuthMapErrorV1::ProofTooLarge));
    assert_eq!(AuthMapProofV1::from_bytes(&[bytes.clone(), vec![0]].concat(), usize::MAX).err(), Some(AuthMapErrorV1::Malformed));
    // A query key longer than the table admits.
    assert_eq!(verify_rows_v1(&SPEC, &m.root(), &honest, &[KeyRangeV1::point(&[0; 81])], BIG).err(), Some(AuthMapErrorV1::KeyTooLong));
}

#[test]
fn another_table_or_version_does_not_verify() {
    let (rows, m) = fixture();
    let k = rows.keys().next().unwrap().clone();
    let p = m.prove_keys(&[&k]);
    for other in [
        AuthTableSpecV1::new(8, 1, 80, 256), // another table
        AuthTableSpecV1::new(7, 2, 80, 256), // another version
        AuthTableSpecV1::new(7, 1, 81, 256), // other limits
    ] {
        assert_eq!(verify_rows_v1(&other, &m.root(), &p, &[KeyRangeV1::point(&k)], BIG).err(), Some(AuthMapErrorV1::RootMismatch));
        // Even against the root of the SAME rows under the other spec: node hashes are tagged, so the proof's opaque hashes differ.
        let mut twin = AuthMapV1::new(other).unwrap();
        for (k, v) in &rows {
            twin.insert(k.clone(), v.clone()).unwrap();
        }
        assert_ne!(twin.root(), m.root());
        assert!(verify_rows_v1(&other, &twin.root(), &p, &[KeyRangeV1::point(&k)], BIG).is_err());
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Updates: old value → new root

#[test]
fn a_verified_update_computes_the_writers_new_root() {
    let mut rng = Rng(8);
    for tight in [false, true] {
        let mut rows = random_rows(&mut rng, if tight { 40 } else { 200 }, tight);
        let mut m = build(&rows);
        for round in 0..60 {
            // a batch of inserts, overwrites and deletes over distinct keys
            let mut batch: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
            for _ in 0..1 + rng.below(8) {
                let k = if rng.below(2) == 0 && !rows.is_empty() {
                    rows.keys().nth(rng.below(rows.len() as u64) as usize).unwrap().clone()
                } else if tight {
                    rng.tight_key()
                } else {
                    rng.bytes_len(1, 30)
                };
                let new = (rng.below(3) != 0).then(|| rng.bytes(4));
                batch.insert(k, new);
            }
            if round == 59 {
                // the last round deletes everything
                batch = rows.keys().map(|k| (k.clone(), None)).collect();
            }
            let updates: Vec<_> =
                batch.iter().map(|(k, new)| RowUpdateV1 { key: k.clone(), old: rows.get(k).cloned(), new: new.clone() }).collect();
            let keys: Vec<_> = batch.keys().cloned().collect();
            let proof = m.prove_keys(&keys);
            let old_root = m.root();
            for u in &updates {
                m.write(u.key.clone(), u.new.clone()).unwrap();
                match &u.new {
                    Some(v) => rows.insert(u.key.clone(), v.clone()),
                    None => rows.remove(&u.key),
                };
            }
            assert_eq!(verify_update_v1(&SPEC, &old_root, &proof, &updates, BIG), Ok(m.root()), "round {round}");
        }
        assert!(m.is_empty());
    }
}

/// Regression (found by the fixture conformance run): a delete whose sibling is opaque, followed in key order by an insert that lands
/// where the deleted leaf was. Applied in key order, the insert would meet the spliced-up opaque sibling and need its prefix.
#[test]
fn a_delete_beside_an_opaque_sibling_and_an_insert_on_its_side_verify() {
    for (a, b, x) in [
        (vec![0x00; 4], vec![0xF0; 4], vec![0x01; 4]), // x on a's side of the root split; a deleted first in key order
        (vec![0x00; 4], vec![0xF0; 4], vec![0xF1; 4]), // x on b's side; b deleted after x
        (vec![0x10; 4], vec![0x80; 4], vec![0x0F; 4]), // x sorts before the deleted key
    ] {
        for deleted in [&a, &b] {
            let mut m = AuthMapV1::new(SPEC).unwrap();
            m.insert(a.clone(), vec![1]).unwrap();
            m.insert(b.clone(), vec![2]).unwrap();
            m.insert(vec![0x42; 9], vec![3]).unwrap();
            let old_root = m.root();
            let old_value = m.get(deleted).map(<[u8]>::to_vec);
            let updates = vec![
                RowUpdateV1 { key: deleted.clone(), old: old_value, new: None },
                RowUpdateV1 { key: x.clone(), old: None, new: Some(vec![9]) },
            ];
            let proof = m.prove_keys(&[deleted.clone(), x.clone()]);
            m.remove(deleted).unwrap();
            m.insert(x.clone(), vec![9]).unwrap();
            assert_eq!(verify_update_v1(&SPEC, &old_root, &proof, &updates, BIG), Ok(m.root()), "{deleted:?} / {x:?}");
        }
    }
}

#[test]
fn a_dishonest_update_is_rejected() {
    let (rows, m) = fixture();
    let (k, v) = rows.iter().nth(7).unwrap();
    let absent = vec![0xEE; 33];
    let proof = m.prove_keys(&[k, &absent]);
    let good = vec![
        RowUpdateV1 { key: k.clone(), old: Some(v.clone()), new: None },
        RowUpdateV1 { key: absent.clone(), old: None, new: Some(vec![1]) },
    ];
    assert!(verify_update_v1(&SPEC, &m.root(), &proof, &good, BIG).is_ok());
    // A stale old value (the writer read something else), or a claimed absence of a present row.
    let mut stale = good.clone();
    stale[0].old = Some(vec![0xAB]);
    assert_eq!(verify_update_v1(&SPEC, &m.root(), &proof, &stale, BIG), Err(AuthMapErrorV1::StaleOldValue));
    let mut hidden = good.clone();
    hidden[0].old = None;
    assert_eq!(verify_update_v1(&SPEC, &m.root(), &proof, &hidden, BIG), Err(AuthMapErrorV1::StaleOldValue));
    // A write the proof does not cover.
    let mut extra = good.clone();
    extra.push(RowUpdateV1 { key: rows.keys().nth(90).unwrap().clone(), old: None, new: Some(vec![]) });
    assert_eq!(verify_update_v1(&SPEC, &m.root(), &proof, &extra, BIG), Err(AuthMapErrorV1::IncompleteRange));
    // The same key twice.
    let mut twice = good.clone();
    twice.push(good[0].clone());
    assert_eq!(verify_update_v1(&SPEC, &m.root(), &proof, &twice, BIG), Err(AuthMapErrorV1::DuplicateWrite));
    // Against another root.
    assert_eq!(verify_update_v1(&SPEC, &h(3), &proof, &good, BIG), Err(AuthMapErrorV1::RootMismatch));
}

// ---------------------------------------------------------------------------------------------------------------------------
// Streams

#[test]
fn a_whole_table_streams_in_bounded_chunks_and_a_skipped_chunk_is_rejected() {
    let (rows, m) = fixture();
    for max_rows in [1usize, 7, 50, 1000] {
        let mut v = AuthStreamVerifierV1::new(SPEC, m.root(), max_rows, BIG);
        let mut position = KeyBoundV1::Unbounded;
        let mut got = Vec::new();
        while !v.is_done() {
            let chunk = m.stream_chunk(&position, max_rows);
            got.extend(v.accept(&chunk).unwrap());
            if let KeyBoundV1::Excluded(k) = &chunk.end {
                position = KeyBoundV1::Included(k.clone());
            }
        }
        assert_eq!(got, rows.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>());
        assert_eq!(v.accept(&m.stream_chunk(&KeyBoundV1::Unbounded, 1)).err(), Some(AuthMapErrorV1::StreamFinished));
    }
    // Skipping the second chunk: the third does not start where the stream stands.
    let mut v = AuthStreamVerifierV1::new(SPEC, m.root(), 10, BIG);
    let c1 = m.stream_chunk(&KeyBoundV1::Unbounded, 10);
    v.accept(&c1).unwrap();
    let KeyBoundV1::Excluded(k1) = &c1.end else { panic!() };
    let c2 = m.stream_chunk(&KeyBoundV1::Included(k1.clone()), 10);
    let KeyBoundV1::Excluded(k2) = &c2.end else { panic!() };
    let c3 = m.stream_chunk(&KeyBoundV1::Included(k2.clone()), 10);
    assert_eq!(v.accept(&c3).err(), Some(AuthMapErrorV1::IncompleteRange));
    // A chunk that claims more than it proves.
    let lying = StreamChunkV1 { end: KeyBoundV1::Unbounded, proof: c2.proof.clone() };
    assert_eq!(v.accept(&lying).err(), Some(AuthMapErrorV1::IncompleteRange));
    // An empty chunk that is not the last makes no progress.
    let empty = StreamChunkV1 { end: KeyBoundV1::Excluded(k1.clone()), proof: m.prove(&[]) };
    assert_eq!(v.accept(&empty).err(), Some(AuthMapErrorV1::NoProgress));
    // Too many rows for the bound.
    let mut tight = AuthStreamVerifierV1::new(SPEC, m.root(), 5, BIG);
    assert_eq!(tight.accept(&c1).err(), Some(AuthMapErrorV1::ProofTooLarge));
}

// ---------------------------------------------------------------------------------------------------------------------------
// The state level: parent root, branch, directory

const BONDS: AuthTableSpecV1 = AuthTableSpecV1::new(1, 1, 68, 4096);
const CLAIMS: AuthTableSpecV1 = AuthTableSpecV1::new(3, 1, 64, 4096);

fn limits() -> AuthProofLimitsV1 {
    AuthProofLimitsV1 { max_table_nodes: BIG, max_directory_nodes: BIG, max_tables: 16 }
}

fn base_state() -> AuthStateV1 {
    let mut s = AuthStateV1::new(h(0), h(100));
    for i in 0..40u8 {
        s.write(&BONDS, vec![i; 68], Some(vec![i, 1])).unwrap();
        s.write(&CLAIMS, vec![i; 64], Some(vec![i, 2])).unwrap();
    }
    s
}

#[test]
fn a_state_row_proof_verifies_only_in_its_own_state_branch_table_and_version() {
    let base = base_state();
    let writes = vec![(BONDS, vec![5u8; 68], Some(vec![9, 9]))];
    let (_, a) = base.child(h(201), &writes).unwrap();
    let (_, b) = base.child(h(202), &writes).unwrap(); // same parent, same writes, another branch
    assert_eq!(a.table(&BONDS).unwrap().root(), b.table(&BONDS).unwrap().root(), "equal tables");
    assert_ne!(a.root(), b.root(), "different branches");

    let k = vec![5u8; 68];
    let pa = a.prove_rows(&BONDS, &[KeyRangeV1::point(&k)]);
    let ok = verify_state_rows_v1(&a.pin(), &BONDS, &pa, &[KeyRangeV1::point(&k)], &limits()).unwrap();
    assert_eq!(ok.get(&k).unwrap(), Some([9u8, 9].as_slice()));

    // Other-branch data: a's proof read under b's pin.
    assert_eq!(
        verify_state_rows_v1(&b.pin(), &BONDS, &pa, &[KeyRangeV1::point(&k)], &limits()).err(),
        Some(AuthMapErrorV1::RootMismatch)
    );
    // a's root but a pin that expects another parent or branch.
    let mut wrong_branch = a.pin();
    wrong_branch.branch_anchor = h(202);
    assert_eq!(
        verify_state_rows_v1(&wrong_branch, &BONDS, &pa, &[KeyRangeV1::point(&k)], &limits()).err(),
        Some(AuthMapErrorV1::OtherBranch)
    );
    let mut wrong_parent = a.pin();
    wrong_parent.parent_state_root = h(7);
    assert_eq!(
        verify_state_rows_v1(&wrong_parent, &BONDS, &pa, &[KeyRangeV1::point(&k)], &limits()).err(),
        Some(AuthMapErrorV1::OtherBranch)
    );
    // The parent's (pre-write) row presented for the child: the header is the parent's.
    let pp = base.prove_rows(&BONDS, &[KeyRangeV1::point(&k)]);
    assert_eq!(
        verify_state_rows_v1(&a.pin(), &BONDS, &pp, &[KeyRangeV1::point(&k)], &limits()).err(),
        Some(AuthMapErrorV1::RootMismatch)
    );
    // Another table: the bonds proof read as claims (the directory names the claims table's root, not this one).
    assert!(verify_state_rows_v1(&a.pin(), &CLAIMS, &pa, &[KeyRangeV1::point(&k)], &limits()).is_err());
    // Another version of the same table: not in the directory.
    let v2 = AuthTableSpecV1::new(1, 2, 68, 4096);
    let p2 = a.prove_rows(&v2, &[KeyRangeV1::point(&k)]);
    assert_eq!(verify_state_rows_v1(&a.pin(), &v2, &p2, &[KeyRangeV1::point(&k)], &limits()).err(), Some(AuthMapErrorV1::TableAbsent));
    // A header that names another format.
    let mut fmt = pa.clone();
    fmt.header.format_version = 2;
    let fmt_pin = AuthStatePinV1 { state_root: fmt.header.root(), ..a.pin() };
    assert_eq!(
        verify_state_rows_v1(&fmt_pin, &BONDS, &fmt, &[KeyRangeV1::point(&k)], &limits()).err(),
        Some(AuthMapErrorV1::WrongFormat)
    );
}

#[test]
fn a_change_set_computes_the_child_root_from_the_parent_pin_alone() {
    let mut rng = Rng(9);
    let mut state = base_state();
    let new_table = AuthTableSpecV1::new(44, 1, 16, 64);
    for round in 0..30u8 {
        let mut writes = Vec::new();
        for _ in 0..1 + rng.below(6) {
            let (spec, len) = match rng.below(3) {
                0 => (BONDS, 68),
                1 => (CLAIMS, 64),
                _ => (new_table, 16),
            };
            let key = vec![rng.below(60) as u8; len];
            writes.push((spec, key, (rng.below(4) != 0).then(|| vec![round])));
        }
        let pin = state.pin();
        let (change, child) = state.child(h(round), &writes).unwrap();
        assert_eq!(verify_state_change_v1(&pin, &change, &limits()), Ok(child.root()), "round {round}");
        // The change set round-trips the wire.
        let bytes = borsh::to_vec(&change).unwrap();
        assert_eq!(borsh::from_slice::<AuthStateChangeSetV1>(&bytes).unwrap(), change);
        // Applied under another parent pin, it is refused.
        let mut other = pin;
        other.branch_anchor = h(250);
        assert_eq!(verify_state_change_v1(&other, &change, &limits()), Err(AuthMapErrorV1::OtherBranch));
        // A forged old value inside a table change is refused.
        if let Some(t) = change.tables.first() {
            let mut forged = change.clone();
            forged.tables[0].updates[0].old = Some(vec![0xFF, 0xFF, 0xFF]);
            assert_ne!(t.updates[0].old, Some(vec![0xFF, 0xFF, 0xFF]));
            assert_eq!(verify_state_change_v1(&pin, &forged, &limits()), Err(AuthMapErrorV1::StaleOldValue));
        }
        // The same table listed twice is refused.
        let mut dup = change.clone();
        dup.tables.push(change.tables[0].clone());
        assert!(verify_state_change_v1(&pin, &dup, &limits()).is_err());
        state = child;
    }
    // Point-proof limits suffice for a change set of point writes.
    let pin = state.pin();
    let (change, child) = state.child(h(1), &[(BONDS, vec![1; 68], None), (CLAIMS, vec![2; 64], Some(vec![]))]).unwrap();
    let bounded = AuthProofLimitsV1::for_points(&BONDS, 2, 2);
    assert_eq!(verify_state_change_v1(&pin, &change, &bounded), Ok(child.root()));
}
