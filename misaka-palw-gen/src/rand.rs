//! **RFC-0003 §I.1 — deterministic randomness `R(seed, domain, step, position, lane)`.**
//!
//! A counter-based function, fixed by the protocol and independent of execution order,
//! parallelism, batching or device count: every lane's value is a pure function of its five
//! arguments, and there is no generator state to advance.
//!
//! ```text
//!   W_d   = words per digest: ⌊512 / b_d⌋ for a blocked domain, 1 for domain 0
//!   block = ⌊lane / W_d⌋,   word = lane mod W_d
//!   D     = BLAKE2b-512( key = K_d, message = enc_d(seed, step, position, block) )
//!   R     = bits [word · b_d, (word + 1) · b_d) of D, read as a big-endian bit string
//!
//!   enc_0 = seed ‖ le32(position) ‖ le64(lane)                      (RFC-0001 D11, byte for byte)
//!   enc_d = seed ‖ le32(step) ‖ le32(position) ‖ le64(block)        (every other domain)
//! ```
//!
//! **Where R lives (RFC-0003 §I.1.1, decided 2026-09-28): at the job layer.** Every input of R is a
//! job fact or a static coordinate, so a random tensor is a job constant: it enters a PALW-TIR
//! program as a *derived input* the court recomputes, and no primitive is added. The seed is the
//! job's committed `seed` — never the job id, which contains the executor-chosen `job_nonce`.
//!
//! **Domain 0 is RFC-0001's D11 sampler, unchanged.** `gumbel_index_v1(seed, position, lane)` in
//! `consensus/core/src/palw_decode_select_v2.rs` is `R(seed, TEXT_GUMBEL_V1, —, position, lane)`;
//! `tests/d11_domain0.rs` proves it against that source and against RFC-0001's golden vectors.
//!
//! **R is never a lottery input** (PALW-RND-6): it decides what an execution computes, and nothing
//! about eligibility.

use blake2b_simd::Params;

/// A job's committed seed (`PalwGenJobV1.seed`; for text, `PalwFreePromptJobV3.sampling_seed`).
pub type Seed = [u8; 32];

/// The per-tensor element cap of PALW-TIR (spec 04b §2.2): no draw is larger than a tensor can be.
pub const RAND_MAX_LANES_V1: u64 = 1 << 28;

/// How a domain lays out its hash message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandLayoutV1 {
    /// Domain 0 (RFC-0001 D11): `seed ‖ le32(position) ‖ le64(lane)`, one word per digest, no step.
    TextGumbel,
    /// Every other domain: `seed ‖ le32(step) ‖ le32(position) ‖ le64(block)`, `⌊512 / b⌋` words per digest.
    Blocked,
}

/// Which `step` coordinate a draw of the domain uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandStepRuleV1 {
    /// Domain 0: there is no step coordinate.
    None,
    /// Always 0 — a one-shot draw (an initial noise).
    Zero,
    /// The consuming stage's scan position `p` (per-step noise of a stochastic sampler).
    PerStep,
    /// Declared per input (`CLASS_UNIFORM_V1`): 0, or the scan position `p`.
    Declared,
}

/// One registered domain (RFC-0003 §I.1.4). A key is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RandDomainV1 {
    pub id: u16,
    pub name: &'static str,
    /// The BLAKE2b key: the ASCII bytes, no length prefix, no terminator, at most 64 bytes.
    pub key: &'static [u8],
    /// `b_d`, the width of one word.
    pub word_bits: u32,
    pub layout: RandLayoutV1,
    pub step: RandStepRuleV1,
}

impl RandDomainV1 {
    /// `W_d`: words one digest yields.
    pub const fn words_per_digest(&self) -> u32 {
        match self.layout {
            RandLayoutV1::TextGumbel => 1,
            RandLayoutV1::Blocked => 512 / self.word_bits,
        }
    }

    /// The largest word, `2^b − 1`.
    pub const fn max_word(&self) -> u32 {
        if self.word_bits >= 32 { u32::MAX } else { (1u32 << self.word_bits) - 1 }
    }
}

pub const TEXT_GUMBEL_V1: u16 = 0;
pub const IMAGE_INIT_NOISE_V1: u16 = 1;
pub const IMAGE_STEP_NOISE_V1: u16 = 2;
pub const AUDIO_INIT_NOISE_V1: u16 = 3;
pub const AUDIO_STEP_NOISE_V1: u16 = 4;
pub const VIDEO_INIT_NOISE_V1: u16 = 5;
pub const VIDEO_STEP_NOISE_V1: u16 = 6;
pub const CLASS_UNIFORM_V1: u16 = 7;

/// **The domain table, RFC-0003 §I.1.4.** Index = id. A new domain is a protocol change: a row
/// here and a new [`rand_set_id_v1`].
pub const RAND_DOMAINS_V1: [RandDomainV1; 8] = [
    RandDomainV1 {
        id: TEXT_GUMBEL_V1,
        name: "TEXT_GUMBEL_V1",
        key: b"misaka-palw/decode-select-v2/gumbel/v1",
        word_bits: 13,
        layout: RandLayoutV1::TextGumbel,
        step: RandStepRuleV1::None,
    },
    RandDomainV1 {
        id: IMAGE_INIT_NOISE_V1,
        name: "IMAGE_INIT_NOISE_V1",
        key: b"misaka-palw/rand/image-init-noise/v1",
        word_bits: 16,
        layout: RandLayoutV1::Blocked,
        step: RandStepRuleV1::Zero,
    },
    RandDomainV1 {
        id: IMAGE_STEP_NOISE_V1,
        name: "IMAGE_STEP_NOISE_V1",
        key: b"misaka-palw/rand/image-step-noise/v1",
        word_bits: 16,
        layout: RandLayoutV1::Blocked,
        step: RandStepRuleV1::PerStep,
    },
    RandDomainV1 {
        id: AUDIO_INIT_NOISE_V1,
        name: "AUDIO_INIT_NOISE_V1",
        key: b"misaka-palw/rand/audio-init-noise/v1",
        word_bits: 16,
        layout: RandLayoutV1::Blocked,
        step: RandStepRuleV1::Zero,
    },
    RandDomainV1 {
        id: AUDIO_STEP_NOISE_V1,
        name: "AUDIO_STEP_NOISE_V1",
        key: b"misaka-palw/rand/audio-step-noise/v1",
        word_bits: 16,
        layout: RandLayoutV1::Blocked,
        step: RandStepRuleV1::PerStep,
    },
    RandDomainV1 {
        id: VIDEO_INIT_NOISE_V1,
        name: "VIDEO_INIT_NOISE_V1",
        key: b"misaka-palw/rand/video-init-noise/v1",
        word_bits: 16,
        layout: RandLayoutV1::Blocked,
        step: RandStepRuleV1::Zero,
    },
    RandDomainV1 {
        id: VIDEO_STEP_NOISE_V1,
        name: "VIDEO_STEP_NOISE_V1",
        key: b"misaka-palw/rand/video-step-noise/v1",
        word_bits: 16,
        layout: RandLayoutV1::Blocked,
        step: RandStepRuleV1::PerStep,
    },
    RandDomainV1 {
        id: CLASS_UNIFORM_V1,
        name: "CLASS_UNIFORM_V1",
        key: b"misaka-palw/rand/class-uniform/v1",
        word_bits: 32,
        layout: RandLayoutV1::Blocked,
        step: RandStepRuleV1::Declared,
    },
];

/// Why a draw was refused. Every refusal is a value, never a panic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandErrorV1 {
    /// No domain has this id.
    UnknownDomain(u16),
    /// `Normal` reads 16-bit words; this domain's words are another width.
    NormalNeedsSixteenBitWords(u16),
    /// More lanes than a PALW-TIR tensor may hold (`2^28`).
    TooManyLanes(u64),
    /// A step other than 0 for a domain whose rule is `Zero` or `None`.
    StepNotAllowed { domain: u16, step: u32 },
}

/// The registered domain `id`.
pub fn rand_domain_v1(id: u16) -> Option<&'static RandDomainV1> {
    RAND_DOMAINS_V1.get(id as usize)
}

/// One digest: `counter` is the lane for domain 0 and the block for every other domain. For domain
/// 0, `step` is not part of the message (the domain has no step coordinate).
pub fn rand_digest_v1(domain: &RandDomainV1, seed: &Seed, step: u32, position: u32, counter: u64) -> [u8; 64] {
    let mut state = Params::new().hash_length(64).key(domain.key).to_state();
    state.update(seed);
    if domain.layout == RandLayoutV1::Blocked {
        state.update(&step.to_le_bytes());
    }
    state.update(&position.to_le_bytes());
    state.update(&counter.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// Word `k` of a digest, `word_bits` wide, the digest read as a big-endian bit string (bit 0 is the
/// most significant bit of byte 0). `None` when the word does not lie inside the 512 bits or the
/// width is not in `1..=32`.
pub fn digest_word_v1(digest: &[u8; 64], word_bits: u32, k: u32) -> Option<u32> {
    if word_bits == 0 || word_bits > 32 {
        return None;
    }
    let start = (k as u64).checked_mul(word_bits as u64)?;
    if start + word_bits as u64 > 512 {
        return None;
    }
    let mut w: u64 = 0;
    for i in 0..word_bits as u64 {
        let bit = (start + i) as usize;
        w = (w << 1) | ((digest[bit / 8] >> (7 - bit % 8)) & 1) as u64;
    }
    Some(w as u32)
}

fn check_step(domain: &RandDomainV1, step: u32) -> Result<(), RandErrorV1> {
    match domain.step {
        RandStepRuleV1::None | RandStepRuleV1::Zero if step != 0 => Err(RandErrorV1::StepNotAllowed { domain: domain.id, step }),
        _ => Ok(()),
    }
}

/// **R itself:** the word of `lane`. For domain 0, `step` must be 0 (it has no step coordinate).
pub fn rand_word_v1(domain: u16, seed: &Seed, step: u32, position: u32, lane: u64) -> Result<u32, RandErrorV1> {
    let d = rand_domain_v1(domain).ok_or(RandErrorV1::UnknownDomain(domain))?;
    check_step(d, step)?;
    let w = d.words_per_digest() as u64;
    let digest = rand_digest_v1(d, seed, step, position, lane / w);
    // The table's widths always fit a digest (13 × 1, 16 × 32, 32 × 16 bits).
    Ok(digest_word_v1(&digest, d.word_bits, (lane % w) as u32).unwrap_or(0))
}

/// Lanes `0..n` of one draw: one digest per block, `W_d` words each. Equal, lane for lane, to
/// [`rand_word_v1`] — which is what makes the order and partition of any evaluation irrelevant.
pub fn rand_words_v1(domain: u16, seed: &Seed, step: u32, position: u32, n: u64) -> Result<Vec<u32>, RandErrorV1> {
    let d = rand_domain_v1(domain).ok_or(RandErrorV1::UnknownDomain(domain))?;
    check_step(d, step)?;
    if n > RAND_MAX_LANES_V1 {
        return Err(RandErrorV1::TooManyLanes(n));
    }
    let w = d.words_per_digest() as u64;
    let mut out = Vec::with_capacity(n as usize);
    let mut block = 0u64;
    while (out.len() as u64) < n {
        let digest = rand_digest_v1(d, seed, step, position, block);
        let take = w.min(n - out.len() as u64) as u32;
        for k in 0..take {
            out.push(digest_word_v1(&digest, d.word_bits, k).unwrap_or(0));
        }
        block += 1;
    }
    Ok(out)
}

/// The value transform of a random input (RFC-0003 §I.1.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandDistV1 {
    /// The word itself: an `idx` in `[0, 2^b − 1]`.
    Uniform,
    /// `PALW_GAUSS_Q24_V1[word]` for a 16-bit word: an `i32` in Q24.
    Normal,
}

/// Lanes `0..n` of a random input's values: the words, or the words through the Gaussian table.
pub fn rand_values_v1(domain: u16, dist: RandDistV1, seed: &Seed, step: u32, position: u32, n: u64) -> Result<Vec<i64>, RandErrorV1> {
    let d = rand_domain_v1(domain).ok_or(RandErrorV1::UnknownDomain(domain))?;
    if dist == RandDistV1::Normal && d.word_bits != 16 {
        return Err(RandErrorV1::NormalNeedsSixteenBitWords(domain));
    }
    let words = rand_words_v1(domain, seed, step, position, n)?;
    Ok(match dist {
        RandDistV1::Uniform => words.into_iter().map(|w| w as i64).collect(),
        RandDistV1::Normal => words.into_iter().map(|w| gauss_q24_v1(w as u16) as i64).collect(),
    })
}

// ---- The Gaussian table ------------------------------------------------------------------------

/// Entries of [`gauss_q24_v1`]: one per 16-bit word.
pub const GAUSS_Q24_V1_LEN: usize = 1 << 16;

/// The key of the table's pin.
pub const GAUSS_Q24_V1_KEY: &[u8] = b"misaka-palw/rand/gauss-q24/v1";

/// **The pin of `PALW_GAUSS_Q24_V1`**: BLAKE2b-512 keyed with [`GAUSS_Q24_V1_KEY`] over the 65,536
/// entries as little-endian `i32` — exactly the bytes of `data/gauss-q24-v1.bin`, as
/// `scripts/palw-gauss-table.py` prints it.
pub const GAUSS_Q24_V1_DIGEST_HEX: &str =
    "0b3c29bd5d81b9dbbaaadfc0d63ce5ab4b017daf99c3af337f97ec7e1c97cc48090db85e4aaea9236a281aca37abfb10446669e8b03e1a2b5923292185285aa4";

/// `PALW_GAUSS_Q24_V1[i] = round_half_away_from_zero(Φ⁻¹((i + ½) / 2^16) · 2^24)`, generated by
/// `scripts/palw-gauss-table.py`. The array's length is part of its type: a file of any other size
/// does not compile.
static GAUSS_Q24_V1_BYTES: &[u8; 4 * GAUSS_Q24_V1_LEN] = include_bytes!("../data/gauss-q24-v1.bin");

/// The smallest entry, `PALW_GAUSS_Q24_V1[0]` (`≈ −4.3249 · 2^24`).
pub const GAUSS_Q24_V1_MIN: i32 = -72_560_101;
/// The largest entry, `PALW_GAUSS_Q24_V1[65535] = −PALW_GAUSS_Q24_V1[0]`.
pub const GAUSS_Q24_V1_MAX: i32 = 72_560_101;

/// **The Normal transform**: the Q24 standard-normal quantile of a 16-bit word's bucket midpoint.
/// Data, not arithmetic: one array index.
pub fn gauss_q24_v1(word: u16) -> i32 {
    let i = word as usize * 4;
    let b = GAUSS_Q24_V1_BYTES;
    i32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// The table's pin, recomputed from the compiled-in bytes.
pub fn gauss_q24_v1_digest() -> [u8; 64] {
    keyed64(GAUSS_Q24_V1_KEY, &[GAUSS_Q24_V1_BYTES.as_slice()])
}

// ---- The rand set --------------------------------------------------------------------------------

/// **ADR-0082 D11's Gumbel table, by its pin** — domain 0's value transform. The table itself lives
/// with the FP decode rule (`PALW_GUMBEL_Q24_V1` in `consensus/core/src/palw_decode_select_v2.rs`);
/// `tests/d11_domain0.rs` checks this is that module's `PALW_GUMBEL_Q24_V1_DIGEST_HEX`.
pub const GUMBEL_Q24_V1_DIGEST_HEX: &str =
    "d2fd7a5c4f4b3a27432233f348087018cde7e1a244d1ffb8c5b700810e2719569b408a9a1383fc3c793a7f47a8aa523f2205a8dd37db1e6cb7e83ee9ae0e1def";

/// The key of [`rand_set_id_v1`].
pub const RAND_SET_ID_KEY_V1: &[u8] = b"misaka-palw/rand-set-id/v1";

/// **The rand-set descriptor**: every domain (id, name, key, width, layout, step rule) and both
/// tables' pins, as one ASCII line. `palw_gen_v1`'s fence value carries its hash, so two builds whose
/// randomness differs in any way have different consensus identities where the fence is armed.
pub fn rand_set_descriptor_v1() -> String {
    let domains: Vec<String> = RAND_DOMAINS_V1
        .iter()
        .map(|d| {
            let layout = match d.layout {
                RandLayoutV1::TextGumbel => "text",
                RandLayoutV1::Blocked => "blocked",
            };
            let step = match d.step {
                RandStepRuleV1::None => "none",
                RandStepRuleV1::Zero => "zero",
                RandStepRuleV1::PerStep => "per-step",
                RandStepRuleV1::Declared => "declared",
            };
            format!("{}:{}:{}:b{}:{layout}:{step}", d.id, d.name, String::from_utf8_lossy(d.key), d.word_bits)
        })
        .collect();
    format!(
        "palw-rand/v1/domains={}/gumbel-q24-v1={GUMBEL_Q24_V1_DIGEST_HEX}/gauss-q24-v1={GAUSS_Q24_V1_DIGEST_HEX}",
        domains.join(",")
    )
}

/// `rand_set_id = BLAKE2b-512(key = "misaka-palw/rand-set-id/v1", rand_set_descriptor_v1())`.
pub fn rand_set_id_v1() -> [u8; 64] {
    keyed64(RAND_SET_ID_KEY_V1, &[rand_set_descriptor_v1().as_bytes()])
}

/// BLAKE2b-512 keyed with `key` over the concatenation of `parts`.
pub(crate) fn keyed64(key: &[u8], parts: &[&[u8]]) -> [u8; 64] {
    let mut state = Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        state.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn the_domain_table_is_indexed_by_id_and_every_key_is_distinct() {
        for (i, d) in RAND_DOMAINS_V1.iter().enumerate() {
            assert_eq!(d.id as usize, i, "{}", d.name);
            assert!(!d.key.is_empty() && d.key.len() <= 64, "{}: a BLAKE2b key is 1..=64 bytes", d.name);
            assert!(d.key.is_ascii(), "{}", d.name);
            assert_eq!(512 % d.word_bits == 0, d.layout == RandLayoutV1::Blocked, "{}: a blocked width divides 512", d.name);
        }
        let mut keys: Vec<&[u8]> = RAND_DOMAINS_V1.iter().map(|d| d.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), RAND_DOMAINS_V1.len(), "a key is never reused");
    }

    #[test]
    fn the_gaussian_table_is_the_pinned_one() {
        assert_eq!(hex(&gauss_q24_v1_digest()), GAUSS_Q24_V1_DIGEST_HEX);
    }

    #[test]
    fn the_gaussian_table_is_increasing_antisymmetric_and_bounded() {
        let t: Vec<i32> = (0..=u16::MAX).map(gauss_q24_v1).collect();
        assert!(t.windows(2).all(|w| w[0] < w[1]), "strictly increasing");
        for i in 0..GAUSS_Q24_V1_LEN {
            assert_eq!(t[GAUSS_Q24_V1_LEN - 1 - i], -t[i], "antisymmetric at {i}");
        }
        assert_eq!(t[0], GAUSS_Q24_V1_MIN);
        assert_eq!(t[GAUSS_Q24_V1_LEN - 1], GAUSS_Q24_V1_MAX);
        assert_eq!((t[32767], t[32768]), (-321, 321), "Φ⁻¹(½ ∓ 2^-17) · 2^24 rounds to ∓321");
        assert_eq!(t.iter().map(|v| *v as i64).sum::<i64>(), 0, "the mean is exactly zero");
        // The second moment of the discrete midpoint distribution is just below 1 (the tails beyond
        // ±4.33σ are cut); within a few parts in 10^4 of 2^48.
        let m2: f64 = t.iter().map(|v| (*v as f64 / (1u64 << 24) as f64).powi(2)).sum::<f64>() / GAUSS_Q24_V1_LEN as f64;
        assert!((0.998..1.0).contains(&m2), "second moment {m2}");
    }

    #[test]
    fn a_word_is_the_same_whether_drawn_alone_or_in_bulk() {
        let seed = [7u8; 32];
        for domain in 1..RAND_DOMAINS_V1.len() as u16 {
            let step = if RAND_DOMAINS_V1[domain as usize].step == RandStepRuleV1::PerStep { 3 } else { 0 };
            let bulk = rand_words_v1(domain, &seed, step, 5, 70).unwrap();
            for (lane, w) in bulk.iter().enumerate() {
                assert_eq!(*w, rand_word_v1(domain, &seed, step, 5, lane as u64).unwrap(), "domain {domain} lane {lane}");
            }
        }
    }

    #[test]
    fn byte_aligned_words_are_the_digest_read_big_endian() {
        let d = [0xA5u8; 64];
        let mut digest = d;
        for (i, b) in digest.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37).wrapping_add(11);
        }
        for k in 0..32 {
            assert_eq!(
                digest_word_v1(&digest, 16, k),
                Some(u16::from_be_bytes([digest[2 * k as usize], digest[2 * k as usize + 1]]) as u32)
            );
        }
        for k in 0..16 {
            let i = 4 * k as usize;
            assert_eq!(
                digest_word_v1(&digest, 32, k),
                Some(u32::from_be_bytes([digest[i], digest[i + 1], digest[i + 2], digest[i + 3]]))
            );
        }
        assert_eq!(digest_word_v1(&digest, 13, 0), Some((u16::from_be_bytes([digest[0], digest[1]]) >> 3) as u32));
        assert_eq!(digest_word_v1(&digest, 16, 32), None, "past the 512 bits");
        assert_eq!(digest_word_v1(&digest, 0, 0), None);
        assert_eq!(digest_word_v1(&digest, 33, 0), None);
    }

    #[test]
    fn refusals_are_values() {
        let seed = [0u8; 32];
        assert_eq!(rand_word_v1(8, &seed, 0, 0, 0), Err(RandErrorV1::UnknownDomain(8)));
        assert_eq!(rand_word_v1(IMAGE_INIT_NOISE_V1, &seed, 1, 0, 0), Err(RandErrorV1::StepNotAllowed { domain: 1, step: 1 }));
        assert_eq!(rand_word_v1(TEXT_GUMBEL_V1, &seed, 1, 0, 0), Err(RandErrorV1::StepNotAllowed { domain: 0, step: 1 }));
        assert_eq!(
            rand_values_v1(CLASS_UNIFORM_V1, RandDistV1::Normal, &seed, 0, 0, 1),
            Err(RandErrorV1::NormalNeedsSixteenBitWords(7))
        );
        assert_eq!(
            rand_words_v1(IMAGE_INIT_NOISE_V1, &seed, 0, 0, RAND_MAX_LANES_V1 + 1),
            Err(RandErrorV1::TooManyLanes(RAND_MAX_LANES_V1 + 1))
        );
        assert!(rand_words_v1(IMAGE_STEP_NOISE_V1, &seed, 9, 0, 1).is_ok());
        assert!(rand_words_v1(CLASS_UNIFORM_V1, &seed, 9, 0, 1).is_ok(), "a declared step may be the scan position");
    }

    #[test]
    fn domains_and_positions_and_steps_separate_streams() {
        let seed = [42u8; 32];
        let a = rand_words_v1(IMAGE_INIT_NOISE_V1, &seed, 0, 0, 64).unwrap();
        let b = rand_words_v1(AUDIO_INIT_NOISE_V1, &seed, 0, 0, 64).unwrap();
        let c = rand_words_v1(IMAGE_INIT_NOISE_V1, &seed, 0, 1, 64).unwrap();
        let d = rand_words_v1(IMAGE_STEP_NOISE_V1, &seed, 0, 0, 64).unwrap();
        let e = rand_words_v1(IMAGE_STEP_NOISE_V1, &seed, 1, 0, 64).unwrap();
        for (x, y) in [(&a, &b), (&a, &c), (&a, &d), (&d, &e)] {
            assert_ne!(x, y);
        }
    }

    #[test]
    fn the_rand_set_descriptor_names_every_domain_and_both_tables() {
        let s = rand_set_descriptor_v1();
        for d in &RAND_DOMAINS_V1 {
            assert!(s.contains(std::str::from_utf8(d.key).unwrap()), "{}", d.name);
        }
        assert!(s.contains(GAUSS_Q24_V1_DIGEST_HEX) && s.contains(GUMBEL_Q24_V1_DIGEST_HEX));
        assert!(s.is_ascii());
    }
}
