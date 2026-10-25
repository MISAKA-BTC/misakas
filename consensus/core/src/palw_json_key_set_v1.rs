//! Auxiliary duplicate-key state for an owned raw JSON stream. This tree authenticates parser
//! transitions from an EMPTY root; it never substitutes for the legacy tokenizer's flat hash.
//! Namespace is the parser's deterministic object sequence number, not a challenger label.
use crate::Hash64;
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_JSON_KEY_HASH_DOMAIN_V1: &[u8] = b"misaka-palw/json/object-key/v1";
pub const PALW_JSON_KEY_SET_DEPTH_V1: usize = 512;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwJsonKeyInsertProofV1 {
    pub namespace: u64,
    pub key_hash: Hash64,
    /// Bottom-to-top, EXACTLY512 siblings. Full512-bit identities are never truncated.
    pub siblings: Vec<Hash64>,
}
/// Canonical transport compression omits known empty siblings without truncating512-bit paths.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwJsonKeyCompactProofV1 {
    pub namespace: u64,
    pub key_hash: Hash64,
    pub non_default: [u8; 64],
    pub siblings: Vec<Hash64>,
}
impl PalwJsonKeyCompactProofV1 {
    pub fn compress(proof: &PalwJsonKeyInsertProofV1) -> Result<Self, String> {
        if proof.siblings.len() != PALW_JSON_KEY_SET_DEPTH_V1 {
            return Err("compact key proof needs exactly512 original siblings".into());
        }
        let mut p = Self { namespace: proof.namespace, key_hash: proof.key_hash, non_default: [0; 64], siblings: Vec::new() };
        for (height, sibling) in proof.siblings.iter().enumerate() {
            if *sibling != defaults()[height] {
                p.non_default[height / 8] |= 1 << (height % 8);
                p.siblings.push(*sibling);
            }
        }
        Ok(p)
    }
    pub fn expand(&self) -> Result<PalwJsonKeyInsertProofV1, String> {
        let count: usize = self.non_default.iter().map(|byte| byte.count_ones() as usize).sum();
        if count != self.siblings.len() {
            return Err("compact key proof count does not match bitmap".into());
        }
        let mut siblings = defaults()[..PALW_JSON_KEY_SET_DEPTH_V1].to_vec();
        let mut cursor = 0;
        for (height, sibling) in siblings.iter_mut().enumerate() {
            if self.non_default[height / 8] & (1 << (height % 8)) != 0 {
                let hash = self.siblings[cursor];
                cursor += 1;
                if hash == *sibling {
                    return Err("compact key proof explicitly repeats a default sibling".into());
                }
                *sibling = hash;
            }
        }
        Ok(PalwJsonKeyInsertProofV1 { namespace: self.namespace, key_hash: self.key_hash, siblings })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwJsonKeySetV1 {
    root: Hash64,
    insertions: u64,
}
fn hash(domain: &[u8], fields: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for field in fields {
        h.update(field);
    }
    Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap())
}
pub fn palw_json_key_hash_v1(decoded_utf8: &[u8]) -> Hash64 {
    hash(PALW_JSON_KEY_HASH_DOMAIN_V1, &[decoded_utf8])
}
pub fn palw_json_key_path_v1(namespace: u64, key_hash: Hash64) -> Hash64 {
    hash(b"misaka-palw/json/object-key-path/v1", &[&namespace.to_le_bytes(), key_hash.as_byte_slice()])
}
fn node(height: usize, left: Hash64, right: Hash64) -> Hash64 {
    hash(b"misaka-palw/json/object-key-node/v1", &[&(height as u16).to_le_bytes(), left.as_byte_slice(), right.as_byte_slice()])
}
fn empty_leaf() -> Hash64 {
    hash(b"misaka-palw/json/object-key-empty/v1", &[])
}
fn present_leaf(path: Hash64) -> Hash64 {
    hash(b"misaka-palw/json/object-key-present/v1", &[path.as_byte_slice()])
}
fn defaults() -> &'static [Hash64] {
    static DEFAULTS: std::sync::OnceLock<Vec<Hash64>> = std::sync::OnceLock::new();
    DEFAULTS.get_or_init(|| {
        let mut rows = vec![empty_leaf()];
        for height in 1..=PALW_JSON_KEY_SET_DEPTH_V1 {
            let last = rows[height - 1];
            rows.push(node(height, last, last));
        }
        rows
    })
}
fn bit(path: &Hash64, index: usize) -> bool {
    (path.as_byte_slice()[index / 8] >> (7 - index % 8)) & 1 != 0
}
fn walk(path: Hash64, mut leaf: Hash64, siblings: &[Hash64]) -> Hash64 {
    for (level, sibling) in siblings.iter().enumerate() {
        let height = level + 1;
        leaf =
            if bit(&path, PALW_JSON_KEY_SET_DEPTH_V1 - height) { node(height, *sibling, leaf) } else { node(height, leaf, *sibling) };
    }
    leaf
}
impl Default for PalwJsonKeySetV1 {
    fn default() -> Self {
        Self { root: defaults()[PALW_JSON_KEY_SET_DEPTH_V1], insertions: 0 }
    }
}
impl PalwJsonKeySetV1 {
    pub fn root(&self) -> Hash64 {
        self.root
    }
    pub fn insertions(&self) -> u64 {
        self.insertions
    }
    /// Authenticated membership/absence verdict. A duplicate marks JSON invalid, but its
    /// surrounding raw tokenizer bytes must still reach their legacy EOF hash before conviction.
    pub fn contains(&self, namespace: u64, key_hash: Hash64, proof: &PalwJsonKeyInsertProofV1) -> Result<bool, String> {
        if proof.namespace != namespace || proof.key_hash != key_hash || proof.siblings.len() != PALW_JSON_KEY_SET_DEPTH_V1 {
            return Err("JSON key proof changes parser namespace/key or exact tree depth".into());
        }
        let path = palw_json_key_path_v1(namespace, key_hash);
        if walk(path, present_leaf(path), &proof.siblings) == self.root {
            return Ok(true);
        }
        if walk(path, empty_leaf(), &proof.siblings) == self.root {
            return Ok(false);
        }
        Err("JSON membership/absence proof is not rooted in this owned stream".into())
    }

    /// Parser supplies the namespace and decoded-key hash it actually reached. A wrong proof,
    /// replay or duplicate is rejected BEFORE mutation; hashes from another object cannot stand in.
    pub fn insert(&mut self, namespace: u64, key_hash: Hash64, proof: &PalwJsonKeyInsertProofV1) -> Result<(), String> {
        if proof.namespace != namespace || proof.key_hash != key_hash || proof.siblings.len() != PALW_JSON_KEY_SET_DEPTH_V1 {
            return Err("JSON duplicate-key proof changes parser namespace/key or exact tree depth".into());
        }
        let count = self.insertions.checked_add(1).ok_or("JSON duplicate-key insertion count overflows")?;
        let path = palw_json_key_path_v1(namespace, key_hash);
        if walk(path, empty_leaf(), &proof.siblings) != self.root {
            return Err("JSON key is duplicated or absence proof is not rooted in this stream".into());
        }
        let root = walk(path, present_leaf(path), &proof.siblings);
        self.root = root;
        self.insertions = count;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn empty_proof(namespace: u64, key: &str) -> PalwJsonKeyInsertProofV1 {
        PalwJsonKeyInsertProofV1 { namespace, key_hash: palw_json_key_hash_v1(key.as_bytes()), siblings: defaults()[..512].to_vec() }
    }
    /// Builds a proof for an empty path in a tree that contains just one other present key.
    fn after_one(namespace: u64, key: &str, first_namespace: u64, first_key: &str) -> PalwJsonKeyInsertProofV1 {
        let target = palw_json_key_path_v1(namespace, palw_json_key_hash_v1(key.as_bytes()));
        let first = palw_json_key_path_v1(first_namespace, palw_json_key_hash_v1(first_key.as_bytes()));
        let divergence = (0..512).find(|index| bit(&target, *index) != bit(&first, *index)).unwrap();
        let height = 511 - divergence;
        let first_subtree = walk(first, present_leaf(first), &defaults()[..height]);
        let mut siblings = defaults()[..512].to_vec();
        siblings[height] = first_subtree;
        PalwJsonKeyInsertProofV1 { namespace, key_hash: palw_json_key_hash_v1(key.as_bytes()), siblings }
    }
    #[test]
    fn duplicate_key_and_foreign_namespace_proofs_refuse_without_mutation_and_restore() {
        let mut set = PalwJsonKeySetV1::default();
        let p = empty_proof(1, "vocab");
        set.insert(1, p.key_hash, &p).unwrap();
        let after = set;
        assert_eq!(set.contains(1, p.key_hash, &p), Ok(true));
        assert!(set.insert(1, p.key_hash, &p).is_err());
        assert_eq!(set, after);
        let p = after_one(2, "vocab", 1, "vocab");
        assert!(set.insert(1, p.key_hash, &p).is_err());
        assert_eq!(set, after);
        set.insert(2, p.key_hash, &p).unwrap();
        assert_eq!(set.insertions(), 2);
        let restored: PalwJsonKeySetV1 = borsh::from_slice(&borsh::to_vec(&set).unwrap()).unwrap();
        assert_eq!(restored, set);
        assert!(borsh::to_vec(&set).unwrap().len() < 128);
    }
    #[test]
    fn compact_proofs_are_full_width_canonical_and_differentially_identical() {
        let empty = empty_proof(1, "model");
        let compact = PalwJsonKeyCompactProofV1::compress(&empty).unwrap();
        assert_eq!(compact.expand().unwrap(), empty);
        assert!(borsh::to_vec(&compact).unwrap().len() < 256);
        let mut set = PalwJsonKeySetV1::default();
        set.insert(1, empty.key_hash, &compact.expand().unwrap()).unwrap();
        let second = after_one(2, "added_tokens", 1, "model");
        let compact = PalwJsonKeyCompactProofV1::compress(&second).unwrap();
        assert_eq!(compact.siblings.len(), 1);
        assert_eq!(compact.expand().unwrap(), second);
        assert_eq!(set.contains(2, second.key_hash, &compact.expand().unwrap()), Ok(false));
        set.insert(2, second.key_hash, &compact.expand().unwrap()).unwrap();
        let mut forged = compact.clone();
        forged.siblings.clear();
        assert!(forged.expand().is_err());
        let mut noncanonical = PalwJsonKeyCompactProofV1::compress(&empty).unwrap();
        noncanonical.non_default[0] = 1;
        noncanonical.siblings.push(defaults()[0]);
        assert!(noncanonical.expand().is_err());
    }
    #[test]
    fn changed_key_path_depth_or_sibling_cannot_insert_and_full_width_is_used() {
        let original = PalwJsonKeySetV1::default();
        let key = palw_json_key_hash_v1(b"model");
        for mutation in 0..4 {
            let mut p = empty_proof(0, "model");
            match mutation {
                0 => p.key_hash = Hash64::default(),
                1 => {
                    p.siblings.pop();
                }
                2 => p.siblings[0] = Hash64::default(),
                _ => p.namespace = 1,
            }
            let mut set = original;
            assert!(set.insert(0, key, &p).is_err());
            assert_eq!(set, original);
        }
        assert_eq!(palw_json_key_path_v1(0, key).as_byte_slice().len() * 8, PALW_JSON_KEY_SET_DEPTH_V1);
        assert!(borsh::to_vec(&empty_proof(0, "model")).unwrap().len() < 33_000);
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_json_key_set_v1.rs"]
mod native_resource_inventory_v1;
