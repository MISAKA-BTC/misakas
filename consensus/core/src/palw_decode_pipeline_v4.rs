//! **RFC-0001 §A — FP Job V4, the deterministic decode pipeline** (Implementation Frozen, G0).
//!
//! The free-prompt lane selects its committed decode token with ADR-0082 Decision 11's seeded
//! argmax ([`crate::palw_decode_select_v2`]). V4 puts four deterministic controls in front of that
//! selection — repeat, frequency/presence, `logit_bias`, stop token sequences — and orders them
//! with the decode constraint (ADR-0096) in ONE total order, so a producer, a panel seat and a
//! court compute the same committed token from the same public facts. The algorithm, the
//! representation and the order below are RFC-0001 §A.2/§A.3 transcribed; nothing here is a code
//! choice, and a change to any of it is an RFC change with new golden vectors
//! (`consensus-vectors/fp-v4/`).
//!
//! ```text
//!   v_j        the engine's integer logit of lane j at decode position t (i32, the class's Q24)
//!   c_j(t)     occurrences of j among the last W = penalty_window GENERATED tokens (never the prompt)
//!
//!   1. repeat     c>0, v>0 : a = floor(v · 65536 / p_q)      c>0, v<=0 : a = floor(v · p_q / 65536)
//!                 c=0      : a = v                            (applied once however large c is)
//!   2. freq/pres  b = a − c·f_q − [c>0]·s_q
//!   3. bias       d = b + bias_j                              (0 without an entry; bans excluded)
//!   4. saturate   v'' = clamp(d, i32::MIN + 1, i32::MAX)
//!   5. mask       A(t) = { j : the constraint admits j } ∖ { j : logit_bias bans j };  empty ⇒ stop
//!   6. select     committed_t = argmax_{j ∈ A(t)} decode_lane_key_v2(v''_j, seed, t, j, T_q)
//!                 ties to the LOWEST index
//!   7. stop       the committed sequence's tail equals a stop sequence ⇒ generation ends here
//!                 (the stop tokens are part of the answer); otherwise continue to the budget
//! ```
//!
//! Every intermediate is an `i64`, every division rounds toward `−∞`, and every input that could
//! overflow is bounded by the canonical form ([`DecodeConfigV4::validate_canonical`]), which a job
//! must pass before anything executes it.
//!
//! **Refutation stays per-lane (I-2).** Steps 1–4 read lane `j`'s own value and quantities the job
//! and the committed prefix fix publicly (`c_j(t)`, `bias_j`), so a court that opens the committed
//! lane's tile and the beating lane's tile, plus the window of generated ids and the job's bias
//! entries, can recompute both keys — [`decode_lane_value_v4`] is that per-lane function, and
//! [`decode_select_v4`] is nothing but its argmax.
//!
//! **Nothing here is armed by its existence.** The fence is `Params::palw_fp_decode_rules`
//! (ADR-0082 D10 + D11 + "V4 required from here"), and a V4 job is admitted only past it.

// ---------------------------------------------------------------------------------------------
// §A.2 — the canonical representation
// ---------------------------------------------------------------------------------------------

/// `1.0` in the repeat penalty's Q16 — the identity, and the only value an inactive penalty has.
pub const PALW_DECODE_V4_REPEAT_Q_ONE: u32 = 1 << 16;
/// The largest repeat penalty a job may carry: `4.0` in Q16.
pub const PALW_DECODE_V4_REPEAT_Q_MAX: u32 = 4 << 16;
/// The widest window a job may count: `W ≤ 256` generated tokens (§A.2).
pub const PALW_DECODE_V4_PENALTY_WINDOW_MAX: u16 = 256;
/// `1.0` in the class's logit unit (Q24, [`crate::palw_base0::K`] fractional bits).
pub const PALW_DECODE_V4_Q24_ONE: i64 = 1i64 << crate::palw_base0::K;
/// `|frequency_penalty_q|, |presence_penalty_q| ≤ 2.0` in Q24 — OpenAI's `[-2, 2]`, mapped.
pub const PALW_DECODE_V4_PENALTY_Q_MAX: i32 = 2 << crate::palw_base0::K;
/// `|bias_q| ≤ 100.0` in Q24 — OpenAI's `[-100, 100]`, mapped.
pub const PALW_DECODE_V4_BIAS_Q_MAX: i32 = 100 << crate::palw_base0::K;
/// `bias_q == −100·2^24` is not a bias: it BANS the lane (a hard mask at step 5).
pub const PALW_DECODE_V4_BIAS_BAN_Q: i32 = -PALW_DECODE_V4_BIAS_Q_MAX;
/// At most 300 `logit_bias` entries.
pub const PALW_DECODE_V4_MAX_BIAS_ENTRIES: usize = 300;
/// At most 4 stop sequences.
pub const PALW_DECODE_V4_MAX_STOP_SEQUENCES: usize = 4;
/// Each stop sequence is 1..=16 token ids.
pub const PALW_DECODE_V4_MAX_STOP_TOKENS: usize = 16;

/// **RFC-0001 §A.2's `DecodeConfigV4`, field for field and in the RFC's order** — the part of an
/// FP Job V4 that decides what the decode pipeline does, and therefore part of the job id.
///
/// **One canonical form per behaviour.** The no-op is exactly [`DecodeConfigV4::NOOP`];
/// `penalty_window` is `1..=256` when any of the three penalties is not its identity and `0`
/// otherwise; `logit_bias` is strictly ascending by token id with no zero entry; `stop_sequences`
/// is strictly ascending in lexicographic order with no empty sequence. Two encodings of one
/// behaviour would be two job ids for one job, so every other encoding is refused BY NAME
/// ([`DecodeConfigV4::validate_canonical`], ADR-0096's principle).
#[derive(Clone, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct DecodeConfigV4 {
    /// Q16 rational `p / 2^16`; `65536` = 1.0 = off. Admissible `[65536, 262144]`.
    pub repeat_penalty_q: u32,
    /// `W`: 0 when every penalty is off, `1..=256` otherwise.
    pub penalty_window: u16,
    /// Q24, in logit units. Admissible `[-2·2^24, 2·2^24]`.
    pub frequency_penalty_q: i32,
    /// Q24, in logit units. Admissible `[-2·2^24, 2·2^24]`.
    pub presence_penalty_q: i32,
    /// `(token_id, bias_q)`, strictly ascending by token id, at most 300, `bias_q` in
    /// `[-100·2^24, 100·2^24] ∖ {0}`; `-100·2^24` bans the lane.
    pub logit_bias: Vec<(u32, i32)>,
    /// Token-id sequences, at most 4, each `1..=16` ids, strictly ascending lexicographically.
    pub stop_sequences: Vec<Vec<u32>>,
}

/// **Why a `DecodeConfigV4` is not canonical** — every refusal names the field and the index, so a
/// gateway can say exactly which parameter of a request was out of form (ADR-0096's principle).
#[derive(thiserror::Error, Clone, Debug, PartialEq, Eq)]
pub enum PalwDecodeConfigV4Error {
    #[error("repeat_penalty_q {got} is outside [65536, 262144] (1.0..=4.0 in Q16)")]
    RepeatPenaltyOutOfRange { got: u32 },
    #[error("frequency_penalty_q {got} is outside [-2·2^24, 2·2^24]")]
    FrequencyPenaltyOutOfRange { got: i32 },
    #[error("presence_penalty_q {got} is outside [-2·2^24, 2·2^24]")]
    PresencePenaltyOutOfRange { got: i32 },
    #[error("penalty_window {got} is outside 1..=256 while a penalty is active")]
    PenaltyWindowOutOfRange { got: u16 },
    #[error("penalty_window {got} is set while every penalty is off — the one canonical form of 'off' is 0")]
    PenaltyWindowWithoutPenalty { got: u16 },
    #[error("logit_bias carries {got} entries; at most 300")]
    TooManyBiasEntries { got: usize },
    #[error("logit_bias entry {index} (token {token}) is not strictly above the previous token id — ascending, no duplicates")]
    BiasNotAscending { index: usize, token: u32 },
    #[error("logit_bias entry {index} (token {token}) carries {got}, outside [-100·2^24, 100·2^24]")]
    BiasOutOfRange { index: usize, token: u32, got: i32 },
    #[error("logit_bias entry {index} (token {token}) is zero — a zero bias is no entry at all")]
    ZeroBias { index: usize, token: u32 },
    #[error("stop_sequences carries {got} sequences; at most 4")]
    TooManyStopSequences { got: usize },
    #[error("stop sequence {index} is empty")]
    EmptyStopSequence { index: usize },
    #[error("stop sequence {index} is {got} tokens long; at most 16")]
    StopSequenceTooLong { index: usize, got: usize },
    #[error("stop sequence {index} is not strictly above the previous one lexicographically — sorted, no duplicates")]
    StopSequencesNotAscending { index: usize },
}

impl DecodeConfigV4 {
    /// **The no-op, and its only encoding** (§A.2): every penalty at its identity, a zero window,
    /// no bias, no stop. A V4 job carrying it decodes exactly what the same V3 job decodes (G7).
    pub const NOOP: Self = Self {
        repeat_penalty_q: PALW_DECODE_V4_REPEAT_Q_ONE,
        penalty_window: 0,
        frequency_penalty_q: 0,
        presence_penalty_q: 0,
        logit_bias: Vec::new(),
        stop_sequences: Vec::new(),
    };

    /// Is this the no-op form?
    pub fn is_noop(&self) -> bool {
        *self == Self::NOOP
    }

    /// Is any of the three penalties away from its identity? Decides whether `penalty_window` is
    /// `1..=256` or `0`.
    pub fn penalties_active(&self) -> bool {
        self.repeat_penalty_q != PALW_DECODE_V4_REPEAT_Q_ONE || self.frequency_penalty_q != 0 || self.presence_penalty_q != 0
    }

    /// **The canonical form of the three penalties and their window** (§A.2).
    pub fn validate_penalties(&self) -> Result<(), PalwDecodeConfigV4Error> {
        if !(PALW_DECODE_V4_REPEAT_Q_ONE..=PALW_DECODE_V4_REPEAT_Q_MAX).contains(&self.repeat_penalty_q) {
            return Err(PalwDecodeConfigV4Error::RepeatPenaltyOutOfRange { got: self.repeat_penalty_q });
        }
        if !(-PALW_DECODE_V4_PENALTY_Q_MAX..=PALW_DECODE_V4_PENALTY_Q_MAX).contains(&self.frequency_penalty_q) {
            return Err(PalwDecodeConfigV4Error::FrequencyPenaltyOutOfRange { got: self.frequency_penalty_q });
        }
        if !(-PALW_DECODE_V4_PENALTY_Q_MAX..=PALW_DECODE_V4_PENALTY_Q_MAX).contains(&self.presence_penalty_q) {
            return Err(PalwDecodeConfigV4Error::PresencePenaltyOutOfRange { got: self.presence_penalty_q });
        }
        if self.penalties_active() {
            if !(1..=PALW_DECODE_V4_PENALTY_WINDOW_MAX).contains(&self.penalty_window) {
                return Err(PalwDecodeConfigV4Error::PenaltyWindowOutOfRange { got: self.penalty_window });
            }
        } else if self.penalty_window != 0 {
            return Err(PalwDecodeConfigV4Error::PenaltyWindowWithoutPenalty { got: self.penalty_window });
        }
        Ok(())
    }
}

/// **`c_j(t)` — how often `lane` occurs among the last `window` GENERATED tokens before position
/// `t = generated.len()`** (§A.3). The prompt is never in `generated`: the window counts the
/// answer only. `window = 0` counts nothing, which is what every inactive penalty carries.
pub fn decode_window_count_v4(generated: &[u32], window: u16, lane: u32) -> u32 {
    let start = generated.len().saturating_sub(window as usize);
    generated[start..].iter().filter(|id| **id == lane).count() as u32
}

/// Every non-zero `c_j(t)` of the window at once, ascending by lane — what one decode step
/// applies to a whole row. At most `W ≤ 256` entries.
pub fn decode_window_counts_v4(generated: &[u32], window: u16) -> Vec<(u32, u32)> {
    let start = generated.len().saturating_sub(window as usize);
    let mut ids: Vec<u32> = generated[start..].to_vec();
    ids.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(ids.len());
    for id in ids {
        match out.last_mut() {
            Some((last, count)) if *last == id => *count += 1,
            _ => out.push((id, 1)),
        }
    }
    out
}

#[cfg(test)]
mod representation_tests {
    use super::*;

    #[test]
    fn the_noop_is_the_one_canonical_off() {
        let noop = DecodeConfigV4::NOOP;
        assert!(noop.is_noop() && !noop.penalties_active());
        noop.validate_penalties().expect("the no-op is canonical");
        // "off" has exactly one spelling: a window with nothing to weigh is a second encoding.
        let windowed = DecodeConfigV4 { penalty_window: 8, ..DecodeConfigV4::NOOP };
        assert_eq!(windowed.validate_penalties(), Err(PalwDecodeConfigV4Error::PenaltyWindowWithoutPenalty { got: 8 }));
    }

    #[test]
    fn an_active_penalty_needs_a_window_of_one_to_256() {
        for (cfg, active) in [
            (DecodeConfigV4 { repeat_penalty_q: 65_537, ..DecodeConfigV4::NOOP }, true),
            (DecodeConfigV4 { frequency_penalty_q: -1, ..DecodeConfigV4::NOOP }, true),
            (DecodeConfigV4 { presence_penalty_q: 1, ..DecodeConfigV4::NOOP }, true),
        ] {
            assert_eq!(cfg.penalties_active(), active);
            assert_eq!(cfg.validate_penalties(), Err(PalwDecodeConfigV4Error::PenaltyWindowOutOfRange { got: 0 }));
            DecodeConfigV4 { penalty_window: 1, ..cfg.clone() }.validate_penalties().expect("W = 1");
            DecodeConfigV4 { penalty_window: 256, ..cfg.clone() }.validate_penalties().expect("W = 256");
            assert_eq!(
                DecodeConfigV4 { penalty_window: 257, ..cfg }.validate_penalties(),
                Err(PalwDecodeConfigV4Error::PenaltyWindowOutOfRange { got: 257 })
            );
        }
    }

    #[test]
    fn the_penalty_ranges_are_the_rfcs() {
        let w = |c: DecodeConfigV4| DecodeConfigV4 { penalty_window: 4, ..c };
        w(DecodeConfigV4 { repeat_penalty_q: 262_144, ..DecodeConfigV4::NOOP }).validate_penalties().expect("4.0");
        assert_eq!(
            w(DecodeConfigV4 { repeat_penalty_q: 262_145, ..DecodeConfigV4::NOOP }).validate_penalties(),
            Err(PalwDecodeConfigV4Error::RepeatPenaltyOutOfRange { got: 262_145 })
        );
        assert_eq!(
            w(DecodeConfigV4 { repeat_penalty_q: 65_535, ..DecodeConfigV4::NOOP }).validate_penalties(),
            Err(PalwDecodeConfigV4Error::RepeatPenaltyOutOfRange { got: 65_535 })
        );
        for q in [-(2 << 24), 2 << 24] {
            w(DecodeConfigV4 { frequency_penalty_q: q, ..DecodeConfigV4::NOOP }).validate_penalties().expect("±2.0");
            w(DecodeConfigV4 { presence_penalty_q: q, ..DecodeConfigV4::NOOP }).validate_penalties().expect("±2.0");
        }
        assert_eq!(
            w(DecodeConfigV4 { frequency_penalty_q: (2 << 24) + 1, ..DecodeConfigV4::NOOP }).validate_penalties(),
            Err(PalwDecodeConfigV4Error::FrequencyPenaltyOutOfRange { got: (2 << 24) + 1 })
        );
        assert_eq!(
            w(DecodeConfigV4 { presence_penalty_q: -(2 << 24) - 1, ..DecodeConfigV4::NOOP }).validate_penalties(),
            Err(PalwDecodeConfigV4Error::PresencePenaltyOutOfRange { got: -(2 << 24) - 1 })
        );
    }

    #[test]
    fn the_window_counts_the_last_w_generated_ids_only() {
        let generated = [7u32, 3, 7, 9, 7, 3];
        assert_eq!(decode_window_count_v4(&generated, 0, 7), 0, "W = 0 counts nothing");
        assert_eq!(decode_window_count_v4(&generated, 2, 7), 1);
        assert_eq!(decode_window_count_v4(&generated, 3, 7), 1);
        assert_eq!(decode_window_count_v4(&generated, 5, 7), 2);
        assert_eq!(decode_window_count_v4(&generated, 256, 7), 3, "a window wider than the answer is the whole answer");
        assert_eq!(decode_window_counts_v4(&generated, 4), vec![(3, 1), (7, 2), (9, 1)]);
        assert_eq!(decode_window_counts_v4(&[], 256), vec![]);
        for w in 0..8u16 {
            let counts = decode_window_counts_v4(&generated, w);
            for lane in 0..12u32 {
                let one = decode_window_count_v4(&generated, w, lane);
                let many = counts.iter().find(|(l, _)| *l == lane).map(|(_, c)| *c).unwrap_or(0);
                assert_eq!(one, many, "the row form and the per-lane form are one count (W {w}, lane {lane})");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// §A.3 step 1 — repeat (multiplicative, applied once)
// ---------------------------------------------------------------------------------------------

/// **Step 1: the repeat penalty of one lane** — `a_j` from `v_j`, `c_j(t)` and `p_q` (§A.3).
///
/// ```text
///   c > 0, v > 0 :  a = floor(v · 65536 / p_q)
///   c > 0, v ≤ 0 :  a = floor(v · p_q / 65536)       (toward −∞: div_euclid by a positive divisor)
///   c = 0        :  a = v
/// ```
///
/// Multiplicative and applied ONCE however many times the lane occurs in the window: a positive
/// logit shrinks toward zero and a non-positive one moves away from it, so a penalty never makes a
/// repeated lane MORE likely. `p_q = 65536` is the identity on both arms. In `i64`: `|v| ≤ 2^31`
/// and `p_q ≤ 2^18` bound both products below `2^50`. A zero `p_q` is not canonical and cannot
/// reach here past admission; it answers `v` rather than dividing by zero, so no input panics.
pub fn decode_repeat_v4(value: i32, count: u32, repeat_penalty_q: u32) -> i64 {
    let v = value as i64;
    if count == 0 || repeat_penalty_q == 0 {
        return v;
    }
    let p = repeat_penalty_q as i64;
    let one = PALW_DECODE_V4_REPEAT_Q_ONE as i64;
    if v > 0 { (v * one) / p } else { (v * p).div_euclid(one) }
}

#[cfg(test)]
mod repeat_tests {
    use super::*;

    #[test]
    fn repeat_is_the_identity_off_the_window_and_at_one() {
        for v in [i32::MIN, -7, -1, 0, 1, 7, i32::MAX] {
            assert_eq!(decode_repeat_v4(v, 0, 4 << 16), v as i64, "c = 0 leaves the lane alone");
            for c in [1, 2, 256] {
                assert_eq!(decode_repeat_v4(v, c, PALW_DECODE_V4_REPEAT_Q_ONE), v as i64, "p = 1.0 is the identity");
            }
        }
    }

    #[test]
    fn repeat_divides_positives_multiplies_non_positives_and_floors() {
        // p = 1.5 (98304): 10 → floor(10·65536/98304) = floor(6.67) = 6; −10 → floor(−15) = −15.
        assert_eq!(decode_repeat_v4(10, 1, 98_304), 6);
        assert_eq!(decode_repeat_v4(-10, 1, 98_304), -15);
        // p = 1.25 (81920): −3 → floor(−3.75) = −4 (toward −∞, not toward zero).
        assert_eq!(decode_repeat_v4(-3, 1, 81_920), -4);
        // v = 0 takes the non-positive arm and stays 0.
        assert_eq!(decode_repeat_v4(0, 3, 262_144), 0);
        // Applied once: the count does not compound it.
        assert_eq!(decode_repeat_v4(1 << 24, 1, 131_072), decode_repeat_v4(1 << 24, 200, 131_072));
        // p = 4.0 at the extremes stays inside i64 and lands where the formula says.
        assert_eq!(decode_repeat_v4(i32::MAX, 1, PALW_DECODE_V4_REPEAT_Q_MAX), (i32::MAX as i64) / 4);
        assert_eq!(decode_repeat_v4(i32::MIN, 1, PALW_DECODE_V4_REPEAT_Q_MAX), (i32::MIN as i64) * 4);
        // A non-canonical zero divisor answers the value rather than panicking.
        assert_eq!(decode_repeat_v4(5, 1, 0), 5);
    }
}

// ---------------------------------------------------------------------------------------------
// §A.3 step 2 — frequency / presence (additive, over the SAME window)
// ---------------------------------------------------------------------------------------------

/// **Step 2: frequency and presence** — `b_j = a_j − c_j(t)·f_q − [c_j(t) > 0]·s_q` (§A.3).
///
/// Over the same window `W` as the repeat penalty (the RFC's decision: OpenAI counts every
/// generated token, and a court that had to count the whole answer would pay for it; bounding the
/// count to `W` bounds the disclosure). `c ≤ 256` and `|f_q|, |s_q| ≤ 2^25` keep every term far
/// inside `i64`.
pub fn decode_frequency_presence_v4(repeated: i64, count: u32, frequency_penalty_q: i32, presence_penalty_q: i32) -> i64 {
    let presence = if count > 0 { presence_penalty_q as i64 } else { 0 };
    repeated - (count as i64) * (frequency_penalty_q as i64) - presence
}

#[cfg(test)]
mod frequency_presence_tests {
    use super::*;

    #[test]
    fn frequency_scales_with_the_count_and_presence_fires_once() {
        let q = 1i64 << 24;
        assert_eq!(decode_frequency_presence_v4(5 * q, 0, 1 << 24, 1 << 24), 5 * q, "absent: untouched");
        assert_eq!(decode_frequency_presence_v4(5 * q, 1, 1 << 24, 1 << 24), 3 * q);
        assert_eq!(decode_frequency_presence_v4(5 * q, 3, 1 << 24, 1 << 24), q);
        assert_eq!(decode_frequency_presence_v4(5 * q, 3, 0, 1 << 24), 4 * q, "presence is a flag, not a count");
        // Negative penalties reward repetition, as OpenAI's do.
        assert_eq!(decode_frequency_presence_v4(0, 2, -(1 << 24), -(1 << 23)), 2 * q + q / 2);
        // The extremes: W = 256 at ±2.0 is ±2^33 — nothing near i64's edge.
        let low = decode_frequency_presence_v4(i32::MIN as i64 * 4, 256, 2 << 24, 2 << 24);
        assert_eq!(low, i32::MIN as i64 * 4 - 256 * (2i64 << 24) - (2i64 << 24));
    }
}

// ---------------------------------------------------------------------------------------------
// §A.3 step 3 — logit_bias (and its bans, which are step 5's)
// ---------------------------------------------------------------------------------------------

/// **What `logit_bias` says about one lane.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwDecodeBiasV4 {
    /// No entry: `bias_j = 0`.
    None,
    /// An additive entry, `bias_q ∈ [-100·2^24, 100·2^24] ∖ {0, -100·2^24}`.
    Add(i32),
    /// `bias_q == -100·2^24`: the lane leaves the admitted set at step 5.
    Ban,
}

impl DecodeConfigV4 {
    /// **The canonical form of `logit_bias`** (§A.2): at most 300 entries, token ids strictly
    /// ascending (sorted, no duplicates), every bias inside `[-100·2^24, 100·2^24]` and non-zero.
    pub fn validate_logit_bias(&self) -> Result<(), PalwDecodeConfigV4Error> {
        if self.logit_bias.len() > PALW_DECODE_V4_MAX_BIAS_ENTRIES {
            return Err(PalwDecodeConfigV4Error::TooManyBiasEntries { got: self.logit_bias.len() });
        }
        let mut previous: Option<u32> = None;
        for (index, &(token, bias)) in self.logit_bias.iter().enumerate() {
            if previous.is_some_and(|p| token <= p) {
                return Err(PalwDecodeConfigV4Error::BiasNotAscending { index, token });
            }
            if !(-PALW_DECODE_V4_BIAS_Q_MAX..=PALW_DECODE_V4_BIAS_Q_MAX).contains(&bias) {
                return Err(PalwDecodeConfigV4Error::BiasOutOfRange { index, token, got: bias });
            }
            if bias == 0 {
                return Err(PalwDecodeConfigV4Error::ZeroBias { index, token });
            }
            previous = Some(token);
        }
        Ok(())
    }

    /// The entry `lane` has, by binary search over the canonical (ascending) list.
    pub fn bias_of(&self, lane: u32) -> PalwDecodeBiasV4 {
        match self.logit_bias.binary_search_by_key(&lane, |(token, _)| *token) {
            Ok(i) => PalwDecodeBiasV4::of_q(self.logit_bias[i].1),
            Err(_) => PalwDecodeBiasV4::None,
        }
    }

    /// Does `logit_bias` ban EVERY lane of a `vocab`-wide row? Then no step can admit a lane, and
    /// a job carrying it could not decode a single token — an executor refuses it before it runs.
    /// (Without a constraint the admitted set is the same at every position, so this is the whole
    /// question of step 5's emptiness.)
    pub fn bans_cover_vocab(&self, vocab: u32) -> bool {
        let banned = self.logit_bias.iter().filter(|(token, bias)| *token < vocab && *bias == PALW_DECODE_V4_BIAS_BAN_Q).count();
        vocab > 0 && banned as u64 >= vocab as u64
    }
}

impl PalwDecodeBiasV4 {
    /// What one canonical `bias_q` means.
    pub fn of_q(bias_q: i32) -> Self {
        if bias_q == PALW_DECODE_V4_BIAS_BAN_Q {
            Self::Ban
        } else if bias_q == 0 {
            Self::None
        } else {
            Self::Add(bias_q)
        }
    }
}

/// **Step 3: the additive bias** — `d_j = b_j + bias_j` (§A.3). Bans never reach here: a banned
/// lane is removed from the admitted set at step 5 and its value is never read.
pub fn decode_bias_v4(penalized: i64, bias: PalwDecodeBiasV4) -> i64 {
    match bias {
        PalwDecodeBiasV4::Add(q) => penalized + q as i64,
        PalwDecodeBiasV4::None | PalwDecodeBiasV4::Ban => penalized,
    }
}

#[cfg(test)]
mod logit_bias_tests {
    use super::*;

    fn with(bias: Vec<(u32, i32)>) -> DecodeConfigV4 {
        DecodeConfigV4 { logit_bias: bias, ..DecodeConfigV4::NOOP }
    }

    #[test]
    fn the_bias_list_is_ascending_bounded_and_non_zero() {
        with(vec![(1, 5), (2, -5), (900, PALW_DECODE_V4_BIAS_BAN_Q), (901, PALW_DECODE_V4_BIAS_Q_MAX)])
            .validate_logit_bias()
            .expect("canonical");
        assert_eq!(
            with(vec![(2, 5), (2, 6)]).validate_logit_bias(),
            Err(PalwDecodeConfigV4Error::BiasNotAscending { index: 1, token: 2 })
        );
        assert_eq!(
            with(vec![(3, 5), (2, 6)]).validate_logit_bias(),
            Err(PalwDecodeConfigV4Error::BiasNotAscending { index: 1, token: 2 })
        );
        assert_eq!(with(vec![(3, 0)]).validate_logit_bias(), Err(PalwDecodeConfigV4Error::ZeroBias { index: 0, token: 3 }));
        assert_eq!(
            with(vec![(3, PALW_DECODE_V4_BIAS_Q_MAX + 1)]).validate_logit_bias(),
            Err(PalwDecodeConfigV4Error::BiasOutOfRange { index: 0, token: 3, got: PALW_DECODE_V4_BIAS_Q_MAX + 1 })
        );
        assert_eq!(
            with(vec![(3, PALW_DECODE_V4_BIAS_BAN_Q - 1)]).validate_logit_bias(),
            Err(PalwDecodeConfigV4Error::BiasOutOfRange { index: 0, token: 3, got: PALW_DECODE_V4_BIAS_BAN_Q - 1 })
        );
        let full: Vec<(u32, i32)> = (0..300).map(|t| (t, 1)).collect();
        with(full.clone()).validate_logit_bias().expect("300 entries");
        let over: Vec<(u32, i32)> = (0..301).map(|t| (t, 1)).collect();
        assert_eq!(with(over).validate_logit_bias(), Err(PalwDecodeConfigV4Error::TooManyBiasEntries { got: 301 }));
    }

    #[test]
    fn a_lane_reads_its_entry_and_a_ban_is_not_a_bias() {
        let cfg = with(vec![(4, 7), (9, PALW_DECODE_V4_BIAS_BAN_Q), (12, -3)]);
        assert_eq!(cfg.bias_of(4), PalwDecodeBiasV4::Add(7));
        assert_eq!(cfg.bias_of(9), PalwDecodeBiasV4::Ban);
        assert_eq!(cfg.bias_of(12), PalwDecodeBiasV4::Add(-3));
        assert_eq!(cfg.bias_of(5), PalwDecodeBiasV4::None);
        assert_eq!(decode_bias_v4(10, PalwDecodeBiasV4::Add(-3)), 7);
        assert_eq!(decode_bias_v4(10, PalwDecodeBiasV4::None), 10);
        // +100 on the largest penalized value stays inside i64 and is saturated at step 4.
        assert_eq!(
            decode_bias_v4(i32::MAX as i64, PalwDecodeBiasV4::Add(PALW_DECODE_V4_BIAS_Q_MAX)),
            i32::MAX as i64 + (100i64 << 24)
        );
    }

    #[test]
    fn bans_cover_a_vocab_only_when_every_lane_is_banned() {
        let ban = PALW_DECODE_V4_BIAS_BAN_Q;
        assert!(with((0..4).map(|t| (t, ban)).collect()).bans_cover_vocab(4));
        assert!(!with((0..4).map(|t| (t, ban)).collect()).bans_cover_vocab(5));
        assert!(!with(vec![(0, ban), (1, ban), (2, ban), (3, 1)]).bans_cover_vocab(4));
        // An entry past the vocabulary bans nothing in it.
        assert!(!with(vec![(0, ban), (1, ban), (2, ban), (7, ban)]).bans_cover_vocab(4));
        assert!(!DecodeConfigV4::NOOP.bans_cover_vocab(0));
    }
}

// ---------------------------------------------------------------------------------------------
// §A.3 step 7 — the stop matcher
// ---------------------------------------------------------------------------------------------

impl DecodeConfigV4 {
    /// **The canonical form of `stop_sequences`** (§A.2): at most 4, each `1..=16` token ids,
    /// strictly ascending lexicographically (sorted, no duplicates).
    pub fn validate_stop_sequences(&self) -> Result<(), PalwDecodeConfigV4Error> {
        if self.stop_sequences.len() > PALW_DECODE_V4_MAX_STOP_SEQUENCES {
            return Err(PalwDecodeConfigV4Error::TooManyStopSequences { got: self.stop_sequences.len() });
        }
        for (index, sequence) in self.stop_sequences.iter().enumerate() {
            if sequence.is_empty() {
                return Err(PalwDecodeConfigV4Error::EmptyStopSequence { index });
            }
            if sequence.len() > PALW_DECODE_V4_MAX_STOP_TOKENS {
                return Err(PalwDecodeConfigV4Error::StopSequenceTooLong { index, got: sequence.len() });
            }
            if index > 0 && self.stop_sequences[index - 1] >= *sequence {
                return Err(PalwDecodeConfigV4Error::StopSequencesNotAscending { index });
            }
        }
        Ok(())
    }
}

/// **Step 7: does the committed answer END with a stop sequence?** — the index of the first stop
/// sequence (in the canonical order) that the tail of `generated` equals, or `None`.
///
/// `generated` is the answer so far INCLUDING the token just committed; the prompt is never part
/// of it, so a stop sequence cannot straddle the prompt. The stop tokens stay in the answer
/// (cutting them from what a user sees is the gateway's display rule). A token id sequence, never
/// a string: a string the tokenizer would split differently in context does not match, which is
/// the documented cost of keeping the tokenizer out of consensus.
pub fn decode_stop_match_v4(stop_sequences: &[Vec<u32>], generated: &[u32]) -> Option<usize> {
    stop_sequences.iter().position(|sequence| !sequence.is_empty() && generated.ends_with(sequence))
}

/// **Where a finished answer's stop rule says it stops** — the length of the shortest prefix of
/// `answer` whose tail is a stop sequence, or `None` when no prefix stops. A committed answer is
/// canonical under step 7 exactly when this is `None` and it ran its whole budget, or when this is
/// its own length (it stopped where the rule says, and not a token later).
pub fn decode_first_stop_v4(stop_sequences: &[Vec<u32>], answer: &[u32]) -> Option<(usize, usize)> {
    (1..=answer.len()).find_map(|end| decode_stop_match_v4(stop_sequences, &answer[..end]).map(|which| (end, which)))
}

#[cfg(test)]
mod stop_tests {
    use super::*;

    fn with(stops: Vec<Vec<u32>>) -> DecodeConfigV4 {
        DecodeConfigV4 { stop_sequences: stops, ..DecodeConfigV4::NOOP }
    }

    #[test]
    fn stop_sequences_are_bounded_non_empty_and_sorted() {
        with(vec![vec![1], vec![1, 2], vec![2], vec![3, 0, 0]]).validate_stop_sequences().expect("canonical");
        assert_eq!(with(vec![vec![]]).validate_stop_sequences(), Err(PalwDecodeConfigV4Error::EmptyStopSequence { index: 0 }));
        assert_eq!(
            with(vec![(0..17).collect()]).validate_stop_sequences(),
            Err(PalwDecodeConfigV4Error::StopSequenceTooLong { index: 0, got: 17 })
        );
        with(vec![(0..16).collect()]).validate_stop_sequences().expect("16 tokens");
        assert_eq!(
            with(vec![vec![2], vec![1]]).validate_stop_sequences(),
            Err(PalwDecodeConfigV4Error::StopSequencesNotAscending { index: 1 })
        );
        assert_eq!(
            with(vec![vec![2], vec![2]]).validate_stop_sequences(),
            Err(PalwDecodeConfigV4Error::StopSequencesNotAscending { index: 1 })
        );
        assert_eq!(
            with(vec![vec![1], vec![2], vec![3], vec![4], vec![5]]).validate_stop_sequences(),
            Err(PalwDecodeConfigV4Error::TooManyStopSequences { got: 5 })
        );
    }

    #[test]
    fn the_tail_matches_and_nothing_else_does() {
        let stops = vec![vec![5, 6], vec![9]];
        assert_eq!(decode_stop_match_v4(&stops, &[1, 5, 6]), Some(0));
        assert_eq!(decode_stop_match_v4(&stops, &[1, 5, 6, 7]), None, "a stop earlier in the answer is not this step's");
        assert_eq!(decode_stop_match_v4(&stops, &[9]), Some(1));
        assert_eq!(decode_stop_match_v4(&stops, &[6]), None, "a partial match is no match");
        assert_eq!(decode_stop_match_v4(&stops, &[]), None);
        // Two sequences where one is the other's suffix: the canonical order decides the index.
        let nested = vec![vec![1, 2], vec![2]];
        assert_eq!(decode_stop_match_v4(&nested, &[1, 2]), Some(0));
        assert_eq!(decode_stop_match_v4(&nested, &[3, 2]), Some(1));
        // The first stop point of a finished answer.
        assert_eq!(decode_first_stop_v4(&stops, &[1, 2, 5, 6, 9]), Some((4, 0)));
        assert_eq!(decode_first_stop_v4(&stops, &[1, 2, 3]), None);
        assert_eq!(decode_first_stop_v4(&[], &[1, 2, 3]), None);
    }
}

