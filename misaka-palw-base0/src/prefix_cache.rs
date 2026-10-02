//! **RFC-0001 §2.6 stage 1 — the KV prefix cache** (node only; no consensus rule reads it).
//!
//! A conversation re-sends its history every turn; a worker that kept the K/V state of a prompt it
//! already prefilled can resume from it instead of walking the shared prefix again. This module is
//! the cache: a key of `(class_id, tokenizer_id, H(prefix ids))`, a value of the K/V snapshot at
//! the end of that prefix, an LRU order and a memory budget the operator sets (`--kv-cache-budget`).
//!
//! # What it is allowed to be used for — and what it is not
//!
//! The committed free-prompt run folds EVERY prefill tile under the job's context hash
//! (`a16_execute_streaming_v1`): a leaf is `H(context ‖ coordinate ‖ tile)`, and the tile is the
//! row the forward pass produced. Skipping a prefix's forward pass would skip the rows the fold
//! must hash, and the cache keeps K/V state, not the ~50 MB-a-position of tiles. So **a committed
//! run never resumes from this cache** (stage 2, the fenced prefix-state receipt, is what removes
//! that cost: the claim then commits only the new range). The cache serves the answer-only path
//! ([`kaspa_consensus_core::palw_backend::PalwExecutionBackendV1::answer_free_prompt_v1`]) — the
//! Chat lane, `--answer-never-commit`, and every answer whose commitment is refused before the run.
//! A committed run still FILLS the cache from its prefill, which costs a copy and no computation.
//!
//! # The golden rule: prefix-resume == fresh
//!
//! The kernels are integer, so a run resumed from a snapshot must be bit-identical to one that
//! walked the prefix itself. That is asserted at three levels: the module's golden test (real
//! engine, every split point), a boot self-check ([`PrefixKvCacheV1::boot_check_v1`] — the worker
//! runs it before serving), and a sampled re-verification of live hits
//! ([`PrefixKvCacheV1::wants_verification_v1`]). The first mismatch DISABLES the cache for the life
//! of the process ([`PrefixKvCacheV1::disable_v1`]): a node that cannot prove the resume equals the
//! fresh run answers from fresh runs, which is slower and never wrong.

use crate::engine_a16::A16Cache;
use kaspa_hashes::Hash64;
use std::collections::BTreeMap;

const DOMAIN: &[u8] = b"misaka-palw/kv-prefix-cache/v1";

/// The prefix key: whose KV this is, under which tokenizer, for which ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrefixKeyV1 {
    pub class_id: Hash64,
    pub tokenizer_id: Hash64,
    pub positions: u32,
    pub prefix_hash: [u8; 32],
}

/// `H(ids[..n])` for every `n` in `lengths` (ascending, each `<= ids.len()`), one pass.
fn prefix_hashes(ids: &[u32], lengths: &[u32]) -> Vec<[u8; 32]> {
    let mut state = blake2b_simd::Params::new().hash_length(32).key(DOMAIN).to_state();
    let mut out = Vec::with_capacity(lengths.len());
    let mut done = 0usize;
    for &n in lengths {
        let n = n as usize;
        for id in &ids[done..n] {
            state.update(&id.to_le_bytes());
        }
        done = n;
        let mut h = [0u8; 32];
        h.copy_from_slice(state.clone().finalize().as_bytes());
        out.push(h);
    }
    out
}

struct Entry {
    cache: A16Cache,
    bytes: u64,
    /// Larger is more recent.
    touched: u64,
    hits: u64,
}

/// What the cache has done, for the operator's log and the response's `misaka.serving` block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrefixCacheStatsV1 {
    pub hits: u64,
    pub misses: u64,
    pub inserts: u64,
    pub evictions: u64,
    pub refused_over_budget: u64,
    pub resident_bytes: u64,
    pub entries: u64,
    pub budget_bytes: u64,
    pub disabled: bool,
}

/// A resume point: the leading `positions` of the prompt are in `cache`.
pub struct PrefixHitV1 {
    pub positions: u32,
    pub cache: A16Cache,
}

pub struct PrefixKvCacheV1 {
    budget_bytes: u64,
    entries: BTreeMap<PrefixKeyV1, Entry>,
    clock: u64,
    resident_bytes: u64,
    stats: PrefixCacheStatsV1,
    disabled: bool,
    /// Re-verify every Nth hit against a fresh prefill (0: never).
    verify_every: u64,
    hits_since_verify: u64,
}

impl PrefixKvCacheV1 {
    /// A cache holding at most `budget_bytes` of K/V snapshots. A budget of zero is a cache that
    /// stores nothing (every lookup misses) — the default of a node that did not ask for one.
    pub fn new(budget_bytes: u64) -> Self {
        Self {
            budget_bytes,
            entries: BTreeMap::new(),
            clock: 0,
            resident_bytes: 0,
            stats: PrefixCacheStatsV1 { budget_bytes, ..Default::default() },
            disabled: false,
            verify_every: 0,
            hits_since_verify: 0,
        }
    }

    pub fn with_verify_every_v1(mut self, n: u64) -> Self {
        self.verify_every = n;
        self
    }

    pub fn enabled_v1(&self) -> bool {
        !self.disabled && self.budget_bytes > 0
    }

    /// After a mismatch: drop everything and never store or serve again.
    pub fn disable_v1(&mut self) {
        self.disabled = true;
        self.entries.clear();
        self.resident_bytes = 0;
        self.stats.disabled = true;
        self.stats.resident_bytes = 0;
        self.stats.entries = 0;
    }

    pub fn stats_v1(&self) -> PrefixCacheStatsV1 {
        let mut s = self.stats;
        s.resident_bytes = self.resident_bytes;
        s.entries = self.entries.len() as u64;
        s
    }

    /// Whether THIS hit should be re-verified against a fresh prefill (every Nth, by the flag).
    pub fn wants_verification_v1(&mut self) -> bool {
        if self.verify_every == 0 {
            return false;
        }
        self.hits_since_verify += 1;
        if self.hits_since_verify >= self.verify_every {
            self.hits_since_verify = 0;
            return true;
        }
        false
    }

    /// **The longest cached prefix of `prompt`** that leaves at least one token to run (the run
    /// needs the last position's logits, which a snapshot does not hold). `None` is a miss.
    pub fn lookup_v1(&mut self, class_id: Hash64, tokenizer_id: Hash64, prompt: &[u32]) -> Option<PrefixHitV1> {
        if !self.enabled_v1() {
            return None;
        }
        // Candidate lengths: every stored length for this (class, tokenizer) that fits.
        let mut lengths: Vec<u32> = self
            .entries
            .keys()
            .filter(|k| k.class_id == class_id && k.tokenizer_id == tokenizer_id && (k.positions as usize) < prompt.len())
            .map(|k| k.positions)
            .collect();
        lengths.sort_unstable();
        lengths.dedup();
        let hashes = prefix_hashes(prompt, &lengths);
        for (n, h) in lengths.iter().zip(hashes.iter()).rev() {
            let key = PrefixKeyV1 { class_id, tokenizer_id, positions: *n, prefix_hash: *h };
            if self.entries.contains_key(&key) {
                self.clock += 1;
                let clock = self.clock;
                let entry = self.entries.get_mut(&key).expect("just found");
                entry.touched = clock;
                entry.hits += 1;
                self.stats.hits += 1;
                return Some(PrefixHitV1 { positions: *n, cache: entry.cache.clone() });
            }
        }
        self.stats.misses += 1;
        None
    }

    /// Store the K/V state after `ids` (the cache holds exactly `ids.len()` rows). Evicts the least
    /// recently used entries to fit; an entry larger than the whole budget is refused and counted.
    pub fn insert_v1(&mut self, class_id: Hash64, tokenizer_id: Hash64, ids: &[u32], cache: &A16Cache) {
        if !self.enabled_v1() || ids.is_empty() {
            return;
        }
        let Some(snapshot) = cache.prefix_clone_v1(ids.len()) else { return };
        let bytes = snapshot.resident_bytes_v1();
        if bytes > self.budget_bytes {
            self.stats.refused_over_budget += 1;
            return;
        }
        let n = ids.len() as u32;
        let prefix_hash = prefix_hashes(ids, &[n])[0];
        let key = PrefixKeyV1 { class_id, tokenizer_id, positions: n, prefix_hash };
        if self.entries.contains_key(&key) {
            self.clock += 1;
            let clock = self.clock;
            self.entries.get_mut(&key).expect("present").touched = clock;
            return;
        }
        while self.resident_bytes + bytes > self.budget_bytes {
            let Some(oldest) = self.entries.iter().min_by_key(|(_, e)| e.touched).map(|(k, _)| *k) else { break };
            if let Some(gone) = self.entries.remove(&oldest) {
                self.resident_bytes -= gone.bytes;
                self.stats.evictions += 1;
            }
        }
        self.clock += 1;
        self.resident_bytes += bytes;
        self.stats.inserts += 1;
        self.entries.insert(key, Entry { cache: snapshot, bytes, touched: self.clock, hits: 0 });
    }

    /// TEST ONLY: corrupt every stored snapshot (what a bad resume would look like).
    #[cfg(test)]
    pub(crate) fn corrupt_all_for_test(&mut self) {
        for entry in self.entries.values_mut() {
            entry.cache.corrupt_first_key_for_test();
        }
    }

    /// The boot self-check: `fresh` and `resumed` are the logits rows (and cache contents) a
    /// fresh prefill and a resumed one produced for the same prompt. Equal, or the cache is off.
    /// Returns whether the cache stays enabled.
    pub fn boot_check_v1(&mut self, fresh: &(Vec<i32>, A16Cache), resumed: &(Vec<i32>, A16Cache)) -> bool {
        if fresh.0 != resumed.0 || fresh.1.contents_as_i32() != resumed.1.contents_as_i32() {
            self.disable_v1();
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_with_rows(rows: usize, fill: i32) -> A16Cache {
        let mut c = A16Cache::with_storage(2, crate::engine_a16::KV_STORAGE_SHIPPED_V1);
        for r in 0..rows {
            for li in 0..2 {
                let row: Vec<i32> = (0..4).map(|k| fill + (r * 4 + k) as i32).collect();
                c.push_key(li, &row).unwrap();
                c.push_value(li, &row).unwrap();
            }
        }
        c
    }

    fn ids(n: usize) -> Vec<u32> {
        (0..n as u32).map(|i| 100 + i).collect()
    }

    #[test]
    fn the_longest_cached_prefix_is_found_and_never_the_whole_prompt() {
        let (class, tok) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let mut cache = PrefixKvCacheV1::new(1 << 20);
        let c8 = cache_with_rows(8, 0);
        cache.insert_v1(class, tok, &ids(4), &c8);
        cache.insert_v1(class, tok, &ids(8), &c8);
        // A prompt that extends the 8-id prefix resumes from 8.
        let hit = cache.lookup_v1(class, tok, &ids(12)).expect("hit");
        assert_eq!(hit.positions, 8);
        assert_eq!(hit.cache.rows(), 8);
        // A prompt that IS the 8-id prefix cannot resume from all of it — one token must run — so 4.
        assert_eq!(cache.lookup_v1(class, tok, &ids(8)).expect("hit").positions, 4);
        // A prompt that diverges inside the prefix does not match it.
        let mut other = ids(12);
        other[6] = 9_999;
        assert_eq!(cache.lookup_v1(class, tok, &other).expect("the 4-prefix still matches").positions, 4);
        other[1] = 9_999;
        assert!(cache.lookup_v1(class, tok, &other).is_none());
        // Another class or tokenizer never shares an entry.
        assert!(cache.lookup_v1(Hash64::from_u64_word(9), tok, &ids(12)).is_none());
        assert!(cache.lookup_v1(class, Hash64::from_u64_word(9), &ids(12)).is_none());
    }

    #[test]
    fn the_budget_evicts_the_least_recently_used_and_refuses_what_never_fits() {
        let (class, tok) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let one = cache_with_rows(4, 0).prefix_clone_v1(4).unwrap().resident_bytes_v1();
        let mut cache = PrefixKvCacheV1::new(one * 2 + one / 2);
        let c = cache_with_rows(4, 0);
        let (a, b, d) = (vec![1u32, 2, 3, 4], vec![5u32, 6, 7, 8], vec![9u32, 10, 11, 12]);
        cache.insert_v1(class, tok, &a, &c);
        cache.insert_v1(class, tok, &b, &c);
        // Touch `a` so `b` is the least recently used.
        assert!(cache.lookup_v1(class, tok, &[1, 2, 3, 4, 99]).is_some());
        cache.insert_v1(class, tok, &d, &c);
        let stats = cache.stats_v1();
        assert_eq!((stats.entries, stats.evictions), (2, 1));
        assert!(stats.resident_bytes <= stats.budget_bytes);
        assert!(cache.lookup_v1(class, tok, &[5, 6, 7, 8, 99]).is_none(), "b was evicted");
        assert!(cache.lookup_v1(class, tok, &[1, 2, 3, 4, 99]).is_some(), "a survived");
        // An entry bigger than the whole budget is refused, not stored.
        let mut tiny = PrefixKvCacheV1::new(one / 2);
        tiny.insert_v1(class, tok, &a, &c);
        assert_eq!(tiny.stats_v1().refused_over_budget, 1);
        assert_eq!(tiny.stats_v1().entries, 0);
        // A zero budget stores nothing and serves nothing.
        let mut off = PrefixKvCacheV1::new(0);
        off.insert_v1(class, tok, &a, &c);
        assert!(off.lookup_v1(class, tok, &[1, 2, 3, 4, 99]).is_none());
    }

    #[test]
    fn a_mismatch_disables_the_cache_for_good() {
        let (class, tok) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let mut cache = PrefixKvCacheV1::new(1 << 20);
        cache.insert_v1(class, tok, &ids(4), &cache_with_rows(4, 0));
        let fresh = (vec![1, 2, 3], cache_with_rows(4, 0));
        let same = (vec![1, 2, 3], cache_with_rows(4, 0));
        assert!(cache.boot_check_v1(&fresh, &same));
        assert!(cache.enabled_v1());
        let different = (vec![1, 2, 4], cache_with_rows(4, 0));
        assert!(!cache.boot_check_v1(&fresh, &different));
        assert!(!cache.enabled_v1());
        assert!(cache.lookup_v1(class, tok, &ids(8)).is_none());
        cache.insert_v1(class, tok, &ids(4), &cache_with_rows(4, 0));
        assert_eq!(cache.stats_v1().entries, 0, "a disabled cache stores nothing");
    }

    #[test]
    fn sampled_verification_fires_on_every_nth_hit() {
        let mut cache = PrefixKvCacheV1::new(1 << 20).with_verify_every_v1(3);
        let fired: Vec<bool> = (0..6).map(|_| cache.wants_verification_v1()).collect();
        assert_eq!(fired, [false, false, true, false, false, true]);
        assert!(!PrefixKvCacheV1::new(1 << 20).wants_verification_v1());
    }
}
