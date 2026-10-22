//! The key alphabet of the authenticated map: a prefix-free, order-preserving bit encoding of raw byte keys, and the
//! "may this subtree hold a key of the range?" test the prover and the verifier share.

use std::cmp::Ordering;
use std::ops::Bound;

/// A bit string, packed most-significant-bit first; the unused low bits of the last byte are always zero (one spelling per
/// string, so a decoded prefix can be compared and hashed byte-for-byte).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct BitPathV1 {
    bytes: Vec<u8>,
    len: u32,
}

/// Bits a raw key of `key_len` bytes encodes to: a continuation bit and eight data bits per byte, and one terminator bit.
pub const fn encoded_bits_v1(key_len: u32) -> u32 {
    key_len * 9 + 1
}

impl BitPathV1 {
    /// **The key encoding.** Each byte `b` becomes `1` followed by the eight bits of `b`; the key ends with `0`. Two properties
    /// follow and the whole design leans on them:
    /// * **prefix-free** — no encoded key is a proper prefix of another (a shorter key ends with `0` where the longer has `1`);
    /// * **order-preserving** — encoded keys compare (bitwise, lexicographically) exactly as the raw keys compare bytewise, with a
    ///   proper prefix first (`[1] < [1, 0] < [2]`).
    pub fn encode_key(key: &[u8]) -> Self {
        let mut path = Self { bytes: Vec::with_capacity((key.len() * 9 + 8) / 8 + 1), len: 0 };
        for &byte in key {
            path.push(1);
            for shift in (0..8).rev() {
                path.push((byte >> shift) & 1);
            }
        }
        path.push(0);
        path
    }

    /// A packed string from the wire: `None` unless `bytes` is exactly `ceil(len / 8)` long with zero padding.
    pub fn from_parts(bytes: Vec<u8>, len: u32) -> Option<Self> {
        if bytes.len() != (len as usize).div_ceil(8) {
            return None;
        }
        let pad = (8 - len % 8) % 8;
        if pad != 0 && bytes.last().is_some_and(|last| last & ((1u8 << pad) - 1) != 0) {
            return None;
        }
        Some(Self { bytes, len })
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn packed(&self) -> &[u8] {
        &self.bytes
    }

    /// Bit `i` (0 or 1). Panics past the end, like an index.
    pub fn bit(&self, i: u32) -> u8 {
        assert!(i < self.len, "bit {i} of a {}-bit path", self.len);
        (self.bytes[(i / 8) as usize] >> (7 - i % 8)) & 1
    }

    pub fn push(&mut self, bit: u8) {
        if self.len % 8 == 0 {
            self.bytes.push(0);
        }
        if bit != 0 {
            let last = self.bytes.len() - 1;
            self.bytes[last] |= 1 << (7 - self.len % 8);
        }
        self.len += 1;
    }

    /// This string followed by one more bit.
    pub fn child(&self, bit: u8) -> Self {
        let mut c = self.clone();
        c.push(bit);
        c
    }

    /// The first `len` bits (canonical: padding zeroed).
    pub fn prefix(&self, len: u32) -> Self {
        assert!(len <= self.len);
        let mut bytes = self.bytes[..(len as usize).div_ceil(8)].to_vec();
        let pad = (8 - len % 8) % 8;
        if pad != 0 {
            let last = bytes.len() - 1;
            bytes[last] &= !((1u8 << pad) - 1);
        }
        Self { bytes, len }
    }

    /// The length of the longest common prefix.
    pub fn common_prefix_len(&self, other: &Self) -> u32 {
        let max = self.len.min(other.len);
        let mut i = 0u32;
        // whole bytes first
        while i + 8 <= max && self.bytes[(i / 8) as usize] == other.bytes[(i / 8) as usize] {
            i += 8;
        }
        while i < max && self.bit(i) == other.bit(i) {
            i += 1;
        }
        i
    }

    pub fn starts_with(&self, prefix: &Self) -> bool {
        prefix.len <= self.len && self.common_prefix_len(prefix) == prefix.len
    }

    /// Where every string that extends `self` lies relative to `bound`.
    pub fn relation_to(&self, bound: &Self) -> PrefixRelationV1 {
        let common = self.common_prefix_len(bound);
        if common < self.len && common < bound.len {
            return if self.bit(common) < bound.bit(common) { PrefixRelationV1::Below } else { PrefixRelationV1::Above };
        }
        match self.len.cmp(&bound.len) {
            Ordering::Less => PrefixRelationV1::PrefixOfBound,
            Ordering::Equal => PrefixRelationV1::Equal,
            Ordering::Greater => PrefixRelationV1::ExtendsBound,
        }
    }
}

/// [`BitPathV1::relation_to`]: the position of the set of strings extending a prefix `Q` relative to a bound `b`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrefixRelationV1 {
    /// `Q` and `b` diverge and `Q` has the `0`: every extension of `Q` is `< b`.
    Below,
    /// `Q` and `b` diverge and `Q` has the `1`: every extension of `Q` is `> b`.
    Above,
    /// `Q` is a proper prefix of `b`: extensions fall on both sides of `b`.
    PrefixOfBound,
    /// `Q == b`.
    Equal,
    /// `b` is a proper prefix of `Q`: every extension of `Q` is `> b`.
    ExtendsBound,
}

/// A range of raw keys, both ends as [`Bound`] (raw keys, compared bytewise; the encoding preserves it).
#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct KeyRangeV1 {
    pub start: KeyBoundV1,
    pub end: KeyBoundV1,
}

/// [`Bound`] over owned raw keys, borsh-encodable (a stream chunk carries its end).
#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum KeyBoundV1 {
    Unbounded,
    Included(Vec<u8>),
    Excluded(Vec<u8>),
}

impl KeyBoundV1 {
    pub fn as_bound(&self) -> Bound<&[u8]> {
        match self {
            Self::Unbounded => Bound::Unbounded,
            Self::Included(k) => Bound::Included(k.as_slice()),
            Self::Excluded(k) => Bound::Excluded(k.as_slice()),
        }
    }

    fn key(&self) -> Option<&[u8]> {
        match self {
            Self::Unbounded => None,
            Self::Included(k) | Self::Excluded(k) => Some(k),
        }
    }
}

impl KeyRangeV1 {
    pub fn all() -> Self {
        Self { start: KeyBoundV1::Unbounded, end: KeyBoundV1::Unbounded }
    }

    /// The one-key range `[key, key]`: a membership or non-membership question.
    pub fn point(key: &[u8]) -> Self {
        Self { start: KeyBoundV1::Included(key.to_vec()), end: KeyBoundV1::Included(key.to_vec()) }
    }

    /// Every key starting with the raw bytes `prefix`: `[prefix, next)` — `next` the shortest key above every extension, or
    /// unbounded when `prefix` is all `0xff`.
    pub fn with_prefix(prefix: &[u8]) -> Self {
        let mut next = prefix.to_vec();
        while let Some(last) = next.pop() {
            if last != 0xff {
                next.push(last + 1);
                return Self { start: KeyBoundV1::Included(prefix.to_vec()), end: KeyBoundV1::Excluded(next) };
            }
        }
        Self { start: KeyBoundV1::Included(prefix.to_vec()), end: KeyBoundV1::Unbounded }
    }

    pub fn contains(&self, key: &[u8]) -> bool {
        let after_start = match &self.start {
            KeyBoundV1::Unbounded => true,
            KeyBoundV1::Included(s) => key >= s.as_slice(),
            KeyBoundV1::Excluded(s) => key > s.as_slice(),
        };
        let before_end = match &self.end {
            KeyBoundV1::Unbounded => true,
            KeyBoundV1::Included(e) => key <= e.as_slice(),
            KeyBoundV1::Excluded(e) => key < e.as_slice(),
        };
        after_start && before_end
    }

    /// The range can hold no key (`start > end`, or an excluded end at or below the start).
    pub fn is_empty(&self) -> bool {
        match (self.start.key(), self.end.key()) {
            (Some(s), Some(e)) => match s.cmp(e) {
                Ordering::Greater => true,
                Ordering::Equal => !matches!((&self.start, &self.end), (KeyBoundV1::Included(_), KeyBoundV1::Included(_))),
                Ordering::Less => false,
            },
            _ => false,
        }
    }

    /// The longest raw key a bound names (a proof-size limit is checked against it).
    pub fn max_bound_len(&self) -> usize {
        self.start.key().map_or(0, <[u8]>::len).max(self.end.key().map_or(0, <[u8]>::len))
    }
}

/// **The completeness rule.** May a subtree whose keys all extend the encoded prefix `q` hold a key of `range`? `false` only
/// when provably not (then a proof may leave the subtree opaque); `true` otherwise (then a proof must open it). Prover and
/// verifier call this one function, so the honest prover opens exactly the subtrees the verifier demands.
pub fn subtree_may_intersect_v1(q: &BitPathV1, range: &KeyRangeV1) -> bool {
    if range.is_empty() {
        return false;
    }
    let below_start = match &range.start {
        KeyBoundV1::Unbounded => false,
        KeyBoundV1::Included(s) | KeyBoundV1::Excluded(s) => q.relation_to(&BitPathV1::encode_key(s)) == PrefixRelationV1::Below,
    };
    let above_end = match &range.end {
        KeyBoundV1::Unbounded => false,
        KeyBoundV1::Included(e) => {
            matches!(q.relation_to(&BitPathV1::encode_key(e)), PrefixRelationV1::Above | PrefixRelationV1::ExtendsBound)
        }
        // An excluded end is also cleared when `q` IS the encoded end: prefix-free, the only key extending it is the end itself.
        KeyBoundV1::Excluded(e) => matches!(
            q.relation_to(&BitPathV1::encode_key(e)),
            PrefixRelationV1::Above | PrefixRelationV1::ExtendsBound | PrefixRelationV1::Equal
        ),
    };
    !(below_start || above_end)
}

/// [`subtree_may_intersect_v1`] over several ranges.
pub fn subtree_may_intersect_any_v1(q: &BitPathV1, ranges: &[KeyRangeV1]) -> bool {
    ranges.iter().any(|r| subtree_may_intersect_v1(q, r))
}
