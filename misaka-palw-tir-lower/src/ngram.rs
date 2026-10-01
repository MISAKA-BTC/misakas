//! **The n-gram hash of `EMBED_NGRAM_PLE_V1`** (Qwen4-Exp's per-layer embedding): pure integer
//! functions, pinned to `Qwen4ExpTextNGramEmbedding` of transformers 5.17.
//!
//! A PLE layer owns `ngram_heads = (ngram_size − 1) · heads_per_ngram` hash heads. For each order
//! `n ∈ 2..=ngram_size`, `heads_per_ngram` heads map the last `n` tokens of the position's segment
//! to a row of that layer's table:
//!
//! ```text
//!   mixed_n = t_0·m_0  XOR  t_1·m_1  XOR … XOR  t_{n−1}·m_{n−1}        (i64)
//!   id      = mixed_n mod size_head + offset_head                       (non-negative modulo)
//! ```
//!
//! * `t_i` is the token `i` positions back **within the segment**: a segment ends at an `eos`
//!   (the `eos` token itself still belongs to the segment it ends), the slots before the start of
//!   the sequence and of a segment read `eos`.
//! * `m_i` are the layer's multipliers: odd, `2 · (splitmix64(base_seed + γ·(i + 1)) mod half_bound) + 1`
//!   with `base_seed = seed + 10007 · layer_index` and `half_bound = max(1, (i64::MAX / vocab) / 2)`,
//!   so `t · m` never leaves `i64` for a valid token id (no wrapping occurs, and none is relied on).
//! * `size_head` is the `(layer_index · ngram_heads + head + 1)`-th prime above `vocab_base − 1`;
//!   `offset_head` is the sum of the sizes of the heads before it.
//!
//! The stream form ([`NgramStream`]) carries a window of the last `ngram_size − 1` *segment-masked*
//! tokens (`W₁(p+1) = t_p`, `W_k(p+1) = W_{k−1}(p)` unless `t_p` is `eos`, then `eos`), which is what
//! a per-position program can keep as state; [`ids_batch`] is the same function written the way
//! transformers writes it (cumulative-maximum `eos` positions over the whole history). The two
//! agree on every sequence (`tests` below), and both agree with the reference vectors that
//! `tools/gen_qwen4_fixtures.py` records from the Hugging Face module.
//!
//! This is the function the **derived input** of a Qwen4-Exp-class program would be (the lowering
//! takes the ids as an input tensor and leaves the table lookup, the gate and the convolution in
//! the program); it is also what a bit-decomposed in-program evaluation is checked against.

use crate::spec::NgramPleSpec;

/// `γ`, `M₁`, `M₂` of splitmix64 and the layer-index prime of the seed.
pub const SPLITMIX_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const SPLITMIX_M1: u64 = 0xBF58_476D_1CE4_E5B9;
const SPLITMIX_M2: u64 = 0x94D0_49BB_1331_11EB;
const PRIME_1: u64 = 10007;

/// splitmix64's finaliser applied to `value + γ`.
pub fn splitmix64(value: u64) -> u64 {
    let v = value.wrapping_add(SPLITMIX_GAMMA);
    let v = (v ^ (v >> 30)).wrapping_mul(SPLITMIX_M1);
    let v = (v ^ (v >> 27)).wrapping_mul(SPLITMIX_M2);
    v ^ (v >> 31)
}

/// Trial division, as the reference.
pub fn is_prime(value: u64) -> bool {
    if value < 2 {
        return false;
    }
    if value % 2 == 0 {
        return value == 2;
    }
    let mut d = 3u64;
    while d * d <= value {
        if value % d == 0 {
            return false;
        }
        d += 2;
    }
    true
}

/// The `count`-th prime strictly above `start`.
pub fn nth_prime_after(start: u64, count: u64) -> u64 {
    let mut prime = start;
    for _ in 0..count {
        prime += 1;
        while !is_prime(prime) {
            prime += 1;
        }
    }
    prime
}

/// A PLE layer's constants (`layer_multipliers`, `ngram_heads_vocab_sizes`, `ngram_heads_offsets`:
/// persistent buffers of the checkpoint, here recomputed from the spec).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NgramTables {
    pub ngram_size: usize,
    pub heads_per_ngram: usize,
    pub eos: i64,
    /// `ngram_size` odd multipliers.
    pub multipliers: Vec<i64>,
    /// One prime per head, `ngram_heads` of them.
    pub head_sizes: Vec<i64>,
    pub head_offsets: Vec<i64>,
    /// Sum of the head sizes.
    pub total_vocab: i64,
    /// `total_vocab` rounded up to a multiple of the divisor: the table's row count.
    pub padded_vocab: i64,
}

impl NgramTables {
    pub fn new(p: &NgramPleSpec) -> NgramTables {
        let ngram_heads = (p.ngram_size - 1) * p.heads_per_ngram;
        // `_build_layer_multipliers`
        let multiplier_max = i64::MAX as u64 / (p.unigram_vocab.max(1) as u64);
        let half_bound = (multiplier_max / 2).max(1);
        let base_seed = p.seed.wrapping_add(PRIME_1.wrapping_mul(p.layer_index as u64));
        let multipliers = (0..p.ngram_size)
            .map(|i| {
                let v = base_seed.wrapping_add(SPLITMIX_GAMMA.wrapping_mul(i as u64 + 1));
                (2 * (splitmix64(v) % half_bound) + 1) as i64
            })
            .collect();
        let mut head_sizes = Vec::with_capacity(ngram_heads);
        let mut head_offsets = Vec::with_capacity(ngram_heads);
        let mut total = 0i64;
        for h in 0..ngram_heads {
            let global = (p.layer_index * ngram_heads + h) as u64;
            let size = nth_prime_after(p.vocab_base as u64 - 1, global + 1) as i64;
            head_sizes.push(size);
            head_offsets.push(total);
            total += size;
        }
        let div = p.vocab_divisor.max(1) as i64;
        NgramTables {
            ngram_size: p.ngram_size,
            heads_per_ngram: p.heads_per_ngram,
            eos: p.eos_id as i64,
            multipliers,
            head_sizes,
            head_offsets,
            total_vocab: total,
            padded_vocab: (total + div - 1) / div * div,
        }
    }

    /// [`NgramTables::new`], memoised by spec: the head sizes are primes found by trial division,
    /// which takes a moment at a vocabulary of tens of millions and is asked for per layer and per
    /// param.
    pub fn cached(p: &NgramPleSpec) -> std::sync::Arc<NgramTables> {
        use std::sync::{Arc, Mutex, OnceLock};
        type Cache = Mutex<std::collections::BTreeMap<String, Arc<NgramTables>>>;
        static CACHE: OnceLock<Cache> = OnceLock::new();
        let key = format!("{p:?}");
        let cache = CACHE.get_or_init(Default::default);
        if let Some(t) = cache.lock().expect("n-gram cache").get(&key) {
            return t.clone();
        }
        let t = Arc::new(NgramTables::new(p));
        cache.lock().expect("n-gram cache").insert(key, t.clone());
        t
    }

    pub fn heads(&self) -> usize {
        self.head_sizes.len()
    }

    /// The ids of every head for a token and the `ngram_size − 1` segment-masked tokens before it
    /// (`window[k − 1]` = the token `k` back, `eos` where the segment has none).
    pub fn ids(&self, token: i64, window: &[i64]) -> Vec<i64> {
        debug_assert_eq!(window.len(), self.ngram_size - 1);
        let mut out = Vec::with_capacity(self.heads());
        for n in 2..=self.ngram_size {
            let mut mixed = token.wrapping_mul(self.multipliers[0]);
            for pos in 1..n {
                mixed ^= window[pos - 1].wrapping_mul(self.multipliers[pos]);
            }
            let start = (n - 2) * self.heads_per_ngram;
            for h in start..start + self.heads_per_ngram {
                // `torch.remainder`: the sign of the divisor (the sizes are positive).
                out.push(mixed.rem_euclid(self.head_sizes[h]) + self.head_offsets[h]);
            }
        }
        out
    }
}

/// The per-position form: the window of the last `ngram_size − 1` segment-masked tokens.
#[derive(Clone, Debug)]
pub struct NgramStream<'t> {
    t: &'t NgramTables,
    window: Vec<i64>,
}

impl<'t> NgramStream<'t> {
    /// A fresh sequence: every slot reads `eos`.
    pub fn new(t: &'t NgramTables) -> Self {
        NgramStream { t, window: vec![t.eos; t.ngram_size - 1] }
    }

    /// The ids at this position, then the window the next position sees.
    pub fn push(&mut self, token: i64) -> Vec<i64> {
        let ids = self.t.ids(token, &self.window);
        let eos = self.t.eos;
        for k in (1..self.window.len()).rev() {
            self.window[k] = if token == eos { eos } else { self.window[k - 1] };
        }
        self.window[0] = token;
        ids
    }

    pub fn window(&self) -> &[i64] {
        &self.window
    }
}

/// `_shift_right_ignore_eos`: `token_ids` shifted right by `shift`, a slot reading `eos` unless the
/// token `shift` back lies in the same segment (no `eos` strictly between, none at the position's
/// own predecessor chain).
fn shift_right_ignore_eos(tokens: &[i64], shift: usize, eos: i64) -> Vec<i64> {
    if shift == 0 {
        return tokens.to_vec();
    }
    let n = tokens.len();
    // the position of the last eos strictly before i (−1: none): the shifted cumulative maximum.
    let mut prev_eos = vec![-1i64; n];
    let mut last = -1i64;
    for i in 0..n {
        prev_eos[i] = last;
        if tokens[i] == eos {
            last = i as i64;
        }
    }
    (0..n)
        .map(|i| {
            let segment_start = prev_eos[i] + 1;
            let in_segment = i as i64 - segment_start;
            let src = i as i64 - shift as i64;
            if in_segment >= shift as i64 && src >= 0 { tokens[src as usize] } else { eos }
        })
        .collect()
}

/// The ids of every position of a sequence, the way transformers computes them: over the history
/// `[eos; ngram_size − 1] ++ tokens`, the last `tokens.len()` positions kept.
pub fn ids_batch(t: &NgramTables, tokens: &[i64]) -> Vec<Vec<i64>> {
    let ctx = t.ngram_size - 1;
    let mut history = vec![t.eos; ctx];
    history.extend_from_slice(tokens);
    let shifted: Vec<Vec<i64>> = (0..t.ngram_size).map(|s| shift_right_ignore_eos(&history, s, t.eos)).collect();
    (ctx..history.len())
        .map(|i| {
            let mut out = Vec::with_capacity(t.heads());
            for n in 2..=t.ngram_size {
                let mut mixed = shifted[0][i].wrapping_mul(t.multipliers[0]);
                for pos in 1..n {
                    mixed ^= shifted[pos][i].wrapping_mul(t.multipliers[pos]);
                }
                let start = (n - 2) * t.heads_per_ngram;
                for h in start..start + t.heads_per_ngram {
                    out.push(mixed.rem_euclid(t.head_sizes[h]) + t.head_offsets[h]);
                }
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Gain, NormKind, NormSpec};

    fn spec(layer_index: usize, ngram_size: usize, heads: usize, vocab: usize, base: usize) -> NgramPleSpec {
        NgramPleSpec {
            ngram_size,
            heads_per_ngram: heads,
            embed_dim: (ngram_size - 1) * heads * 4,
            conv_kernel: 4,
            conv_dilation: ngram_size,
            norm: NormSpec { kind: NormKind::Rms, eps: 1e-6, gain: Gain::OnePlusW, bias: false },
            eos_id: 2,
            seed: 1234,
            vocab_base: base,
            vocab_divisor: 128,
            layer_index,
            unigram_vocab: vocab,
        }
    }

    #[test]
    fn primes_and_splitmix_are_the_references() {
        assert!(is_prime(2) && is_prime(3) && is_prime(97) && !is_prime(1) && !is_prime(91) && is_prime(10007));
        // the 1st, 2nd and 3rd prime above 19_999_999
        assert_eq!(nth_prime_after(19_999_999, 1), 20_000_003);
        assert_eq!(nth_prime_after(19_999_999, 2), 20_000_023);
        assert_eq!(nth_prime_after(19_999_999, 3), 20_000_033);
        // splitmix64 of 0 (the published first output of the generator seeded with 0)
        assert_eq!(splitmix64(0), 0xE220_A839_7B1D_CDAF);
    }

    #[test]
    fn the_multipliers_are_odd_and_never_overflow_a_token() {
        for layer in 0..4 {
            let s = spec(layer, 3, 8, 248_320, 20_000_000);
            let t = NgramTables::new(&s);
            assert_eq!(t.multipliers.len(), 3);
            for m in &t.multipliers {
                assert!(m % 2 == 1 && *m > 0);
                assert!((248_319i128 * *m as i128) <= i64::MAX as i128, "a valid token times a multiplier fits i64");
            }
            assert_eq!(t.head_sizes.len(), 16);
            assert_eq!(t.padded_vocab % 128, 0);
        }
    }

    #[test]
    fn the_stream_is_the_transformers_function() {
        // every window length, sequences with eos runs, eos at the start and at the end
        let mut x = 0x1234_5678_9abc_def0u64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for (size, heads, vocab, base) in [(2usize, 2usize, 16usize, 31usize), (3, 3, 64, 101), (4, 2, 64, 53), (5, 1, 64, 211)] {
            for layer in 0..3 {
                let t = NgramTables::new(&spec(layer, size, heads, vocab, base));
                for _ in 0..40 {
                    let len = 1 + (next() % 24) as usize;
                    // eos (=2) common enough to land in the middle of an n-gram
                    let tokens: Vec<i64> = (0..len).map(|_| if next() % 4 == 0 { 2 } else { (next() % vocab as u64) as i64 }).collect();
                    let batch = ids_batch(&t, &tokens);
                    let mut s = NgramStream::new(&t);
                    for (i, tok) in tokens.iter().enumerate() {
                        assert_eq!(s.push(*tok), batch[i], "size {size} layer {layer} tokens {tokens:?} position {i}");
                    }
                }
            }
        }
    }

    #[test]
    fn an_eos_ends_the_segment_and_the_hash_sees_only_the_segment() {
        let t = NgramTables::new(&spec(0, 3, 2, 64, 101));
        let a = ids_batch(&t, &[5, 6, 7, 2, 9, 11]);
        let b = ids_batch(&t, &[40, 41, 42, 2, 9, 11]);
        // after the eos the history is the same segment ([9, 11]), whatever came before it
        assert_eq!(a[4], b[4]);
        assert_eq!(a[5], b[5]);
        // but the eos position itself still sees its own segment's tokens
        assert_ne!(a[3], b[3]);
    }
}
