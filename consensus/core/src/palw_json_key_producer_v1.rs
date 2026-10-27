//! Private sparse DATA authentication tree. Not a computation validity proof.
use super::*;
use std::collections::BTreeMap;

pub(crate) struct ProducerTree {
    nodes: BTreeMap<(usize, [u8; 64]), Hash64>,
    pub(crate) hash_calls: u64,
    pub(crate) hash_bytes: u64,
    max_calls: u64,
    max_bytes: u64,
}
impl ProducerTree {
    pub(crate) fn new(max_calls: u64, max_bytes: u64) -> Self {
        Self { nodes: BTreeMap::new(), hash_calls: 0, hash_bytes: 0, max_calls, max_bytes }
    }
    pub(crate) fn root(&self) -> Hash64 {
        self.nodes.get(&(512, [0; 64])).copied().unwrap_or(defaults()[512])
    }
    pub(crate) fn nodes(&self) -> usize {
        self.nodes.len()
    }
    fn charge(&mut self, bytes: u64) -> Result<(), &'static str> {
        let calls = self.hash_calls.checked_add(1).ok_or("hash count overflow")?;
        let total = self.hash_bytes.checked_add(bytes).ok_or("hash byte overflow")?;
        if calls > self.max_calls || total > self.max_bytes {
            return Err("local generator hash budget exhausted");
        }
        self.hash_calls = calls;
        self.hash_bytes = total;
        Ok(())
    }
    pub(crate) fn proof(&mut self, namespace: u64, key_hash: Hash64) -> Result<(Hash64, PalwJsonKeyCompactProofV1), &'static str> {
        self.charge(72)?;
        let path = palw_json_key_path_v1(namespace, key_hash);
        let mut proof = PalwJsonKeyCompactProofV1 { namespace, key_hash, non_default: [0; 64], siblings: Vec::new() };
        // Reserved by the Court owner before this allocation; capacity never grows.
        proof.siblings.try_reserve_exact(512).map_err(|_| "compact sibling allocation refused")?;
        for h in 0..512 {
            let mut other = prefix(path, h);
            let bit = 511 - h;
            other[bit / 8] ^= 1 << (7 - bit % 8);
            let sibling = self.nodes.get(&(h, other)).copied().unwrap_or(defaults()[h]);
            if sibling != defaults()[h] {
                proof.non_default[h / 8] |= 1 << (h % 8);
                proof.siblings.push(sibling);
            }
        }
        Ok((path, proof))
    }
    pub(crate) fn insert_path(&mut self, path: Hash64) -> Result<(), &'static str> {
        // Charge the entire update before mutation, avoiding a partial successful tree.
        let calls = self.hash_calls.checked_add(513).ok_or("hash count overflow")?;
        let bytes = self.hash_bytes.checked_add(64 + 512 * 130).ok_or("hash byte overflow")?;
        if calls > self.max_calls || bytes > self.max_bytes {
            return Err("local generator hash budget exhausted");
        }
        self.hash_calls = calls;
        self.hash_bytes = bytes;
        self.nodes.insert((0, prefix(path, 0)), present_leaf(path));
        for h in 1..=512 {
            let left = prefix(path, h);
            let mut right = left;
            let bit = 512 - h;
            right[bit / 8] |= 1 << (7 - bit % 8);
            let l = self.nodes.get(&(h - 1, left)).copied().unwrap_or(defaults()[h - 1]);
            let r = self.nodes.get(&(h - 1, right)).copied().unwrap_or(defaults()[h - 1]);
            self.nodes.insert((h, left), node(h, l, r));
        }
        Ok(())
    }
}
fn prefix(path: Hash64, height: usize) -> [u8; 64] {
    let mut out = *path.as_byte_slice();
    for b in 512 - height..512 {
        out[b / 8] &= !(1 << (7 - b % 8));
    }
    out
}
