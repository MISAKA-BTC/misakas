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

// ---------------------------------------------------------------------------------------------
// §A.3 — the processor's total order: 1 → 2 → 3 → 4 → 5 → 6 → 7
// ---------------------------------------------------------------------------------------------

use crate::palw_decode_select_v2::{PalwDecodeSamplingV2, decode_lane_beats_v2, decode_lane_key_v2, decode_token_select_v2};

impl DecodeConfigV4 {
    /// **The whole canonical form** (§A.2): the penalties and their window, `logit_bias`, and
    /// `stop_sequences`, refused by name in that order. A job must pass this before anything
    /// executes it, and every overflow bound this module relies on is one of its checks.
    pub fn validate_canonical(&self) -> Result<(), PalwDecodeConfigV4Error> {
        self.validate_penalties()?;
        self.validate_logit_bias()?;
        self.validate_stop_sequences()
    }
}

/// **Step 4: saturation** — `v'' = clamp(d, i32::MIN + 1, i32::MAX)` (§A.3). `i32::MIN` itself is
/// never a processed value.
pub fn decode_saturate_v4(value: i64) -> i32 {
    value.clamp(i32::MIN as i64 + 1, i32::MAX as i64) as i32
}

/// **Steps 1–4 for ONE lane** — the per-lane function a court recomputes from one opened tile:
/// `v''_j` from the lane's engine value `v_j`, its window count `c_j(t)` and the job's config, or
/// `None` when `logit_bias` bans the lane (it is not in `A(t)`, and its value is never read).
pub fn decode_lane_value_v4(config: &DecodeConfigV4, value: i32, lane: u32, count: u32) -> Option<i32> {
    let bias = config.bias_of(lane);
    if bias == PalwDecodeBiasV4::Ban {
        return None;
    }
    let repeated = decode_repeat_v4(value, count, config.repeat_penalty_q);
    let penalized = decode_frequency_presence_v4(repeated, count, config.frequency_penalty_q, config.presence_penalty_q);
    Some(decode_saturate_v4(decode_bias_v4(penalized, bias)))
}

/// **One lane's selection key under V4** — [`decode_lane_value_v4`] then ADR-0082 Decision 11's
/// [`decode_lane_key_v2`] at position `t = generated_before.len()`. `None` for a banned lane.
/// This and [`decode_lane_beats_v2`] are everything a two-tile refutation needs.
pub fn decode_lane_key_v4(
    config: &DecodeConfigV4,
    sampling: &PalwDecodeSamplingV2,
    generated_before: &[u32],
    value: i32,
    lane: u32,
) -> Option<i64> {
    let count = decode_window_count_v4(generated_before, config.penalty_window, lane);
    let processed = decode_lane_value_v4(config, value, lane, count)?;
    Some(decode_lane_key_v2(processed, &sampling.seed, generated_before.len() as u32, lane as usize, sampling.temperature_q))
}

/// **Steps 1–6 over a whole row: the committed lane at position `t = generated_before.len()`**, or
/// `None` when `A(t)` is empty (every lane the constraint admits is banned — generation ends at
/// this step, §A.3 step 5).
///
/// `admitted` is the decode constraint's mask (ADR-0096); a job without a constraint passes
/// `|_| true`. The mask is the LAST filter before selection, after every value correction, and a
/// `logit_bias` ban is part of it — so the order is repeat → frequency/presence → bias →
/// saturation → mask (constraint ∖ bans) → Decision 11's key → argmax, ties to the lowest index.
/// The value corrections read only `(v_j, c_j(t), bias_j)`, so this is exactly the argmax of
/// [`decode_lane_key_v4`] over the admitted lanes; it merges the window's counts and the bias list
/// (both ascending) against the row rather than searching per lane.
pub fn decode_select_v4(
    config: &DecodeConfigV4,
    sampling: &PalwDecodeSamplingV2,
    generated_before: &[u32],
    values: &[i32],
    admitted: &dyn Fn(usize) -> bool,
) -> Option<usize> {
    let position = generated_before.len() as u32;
    let counts = decode_window_counts_v4(generated_before, config.penalty_window);
    let (mut next_count, mut next_bias) = (0usize, 0usize);
    let mut best: Option<(usize, i64)> = None;
    for (lane, value) in values.iter().enumerate() {
        let lane_id = lane as u32;
        while next_count < counts.len() && counts[next_count].0 < lane_id {
            next_count += 1;
        }
        while next_bias < config.logit_bias.len() && config.logit_bias[next_bias].0 < lane_id {
            next_bias += 1;
        }
        let count = match counts.get(next_count) {
            Some((id, count)) if *id == lane_id => *count,
            _ => 0,
        };
        let bias = match config.logit_bias.get(next_bias) {
            Some((id, bias_q)) if *id == lane_id => PalwDecodeBiasV4::of_q(*bias_q),
            _ => PalwDecodeBiasV4::None,
        };
        if bias == PalwDecodeBiasV4::Ban || !admitted(lane) {
            continue;
        }
        let repeated = decode_repeat_v4(*value, count, config.repeat_penalty_q);
        let penalized = decode_frequency_presence_v4(repeated, count, config.frequency_penalty_q, config.presence_penalty_q);
        let processed = decode_saturate_v4(decode_bias_v4(penalized, bias));
        let key = decode_lane_key_v2(processed, &sampling.seed, position, lane, sampling.temperature_q);
        best = match best {
            Some((best_lane, best_key)) if !decode_lane_beats_v2(key, lane, best_key, best_lane) => Some((best_lane, best_key)),
            _ => Some((lane, key)),
        };
    }
    best.map(|(lane, _)| lane)
}

/// **Why a free-prompt run stopped where it did** — the reason the processor records (§A.3 steps
/// 5 and 7, and the budget). Derived, never declared: the commitment carries only the canonical
/// `PalwFpStopReasonV3` (`ExactBudgetReached` iff the budget was used up), and this finer reason is
/// a pure function of the job and the committed answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PalwFpDecodeStopReasonV1 {
    /// `decode_token_limit` tokens were committed and no stop sequence ended the answer earlier.
    Budget,
    /// The answer's tail equals `stop_sequences[index]` (the stop tokens are part of the answer).
    StopSequence { index: u8 },
    /// Step 5's admitted set was empty: every lane the constraint admits is banned.
    NoAdmissibleLane,
}

/// Where a run stopped, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwFpDecodeStopV1 {
    /// Committed decode tokens — the claim's `decode_tokens_executed`.
    pub executed: u32,
    pub reason: PalwFpDecodeStopReasonV1,
}

/// **The one decoder every engine, seat and replay drives** — the V3 rule for a V3 job, the §A.3
/// pipeline for a V4 job, one step per committed logits row, in row order.
///
/// An engine hands it each selecting row (row `t` is the row decode token `t` is chosen from:
/// the last prefill position's row for `t = 0`, decode call `t`'s row after that) and feeds the
/// returned lane into the next forward pass. The decoder keeps the committed answer and records
/// the FIRST stop — a stop sequence completed (§A.3 step 7), an empty admitted set (step 5), or the
/// budget. An engine whose capture must know its length before it begins can run once to the
/// budget, read [`Self::stop`], and run again to exactly that length: selection depends only on
/// the committed prefix, so the second run commits the same tokens. Past a stop the decoder keeps
/// answering (Decision 11's rule over the raw row) so a fixed-length loop can finish; those lanes
/// are never committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFpDecoderV1 {
    /// `None` is the V3 rule: Decision 11's key over the raw row, no stop before the budget.
    config: Option<DecodeConfigV4>,
    sampling: PalwDecodeSamplingV2,
    limit: u32,
    generated: Vec<u32>,
    stop: Option<PalwFpDecodeStopV1>,
}

impl PalwFpDecoderV1 {
    /// The V3 rule: `decode_token_select_v2` under the job's sampling pair (the plain argmax at
    /// the greedy temperature), to the budget. EOG is a display stop on this rule.
    pub fn v3(sampling: PalwDecodeSamplingV2, limit: u32) -> Self {
        Self { config: None, sampling, limit, generated: Vec::new(), stop: None }
    }

    /// The §A.3 pipeline under `config` (which the caller has validated as canonical).
    pub fn v4(config: DecodeConfigV4, sampling: PalwDecodeSamplingV2, limit: u32) -> Self {
        Self { config: Some(config), sampling, limit, generated: Vec::new(), stop: None }
    }

    /// **Resume after a committed prefix** — a replay that starts at position `history.len()`
    /// (a seat's interval, a court's position) is exactly the decoder that committed `history`.
    /// The stop rule is re-read over the prefix, so a history that already stopped is stopped.
    pub fn resumed(mut self, history: &[u32]) -> Self {
        for id in history {
            if self.stop.is_some() {
                break;
            }
            self.commit(*id);
        }
        self
    }

    /// Is this the §A.3 pipeline (a V4 job)?
    pub fn is_v4(&self) -> bool {
        self.config.is_some()
    }

    /// The committed answer so far (never past the first stop).
    pub fn generated(&self) -> &[u32] {
        &self.generated
    }

    /// The first stop, once one has happened.
    pub fn stop(&self) -> Option<PalwFpDecodeStopV1> {
        self.stop
    }

    /// **One step**: the lane to feed the engine's next forward pass, selected from `row`.
    pub fn select(&mut self, row: &[i32]) -> u32 {
        let position = self.generated.len() as u32;
        let sampling = self.sampling;
        let raw = move || decode_token_select_v2(row, &sampling.seed, position, sampling.temperature_q) as u32;
        if self.stop.is_some() {
            return raw();
        }
        let Some(config) = &self.config else {
            let lane = raw();
            self.commit(lane);
            return lane;
        };
        match decode_select_v4(config, &self.sampling, &self.generated, row, &|_| true) {
            Some(lane) => {
                let lane = lane as u32;
                self.commit(lane);
                lane
            }
            None => {
                self.stop = Some(PalwFpDecodeStopV1 { executed: position, reason: PalwFpDecodeStopReasonV1::NoAdmissibleLane });
                raw()
            }
        }
    }

    fn commit(&mut self, lane: u32) {
        self.generated.push(lane);
        if let Some(config) = &self.config
            && let Some(index) = decode_stop_match_v4(&config.stop_sequences, &self.generated)
        {
            self.stop = Some(PalwFpDecodeStopV1 {
                executed: self.generated.len() as u32,
                reason: PalwFpDecodeStopReasonV1::StopSequence { index: index as u8 },
            });
            return;
        }
        if self.generated.len() as u64 >= self.limit as u64 {
            self.stop = Some(PalwFpDecodeStopV1 { executed: self.generated.len() as u32, reason: PalwFpDecodeStopReasonV1::Budget });
        }
    }
}

/// **Is a committed answer where the stop rule ends it?** — step 7 checked over a whole answer:
/// no proper prefix ends in a stop sequence, and the answer either ends in one or runs the whole
/// budget; and step 5 checked for the bans (no committed id is banned, and a run shorter than its
/// budget that ends in no stop sequence is admissible only when the admitted set was empty).
/// Pure in the job and the answer, so a seat, a court and a gateway ask it the same way. Returns
/// the stop the answer encodes.
pub fn decode_answer_stop_v4(
    config: &DecodeConfigV4,
    limit: u32,
    vocab: u32,
    answer: &[u32],
) -> Result<PalwFpDecodeStopV1, &'static str> {
    if answer.is_empty() {
        return Err("an answer of no tokens decoded nothing");
    }
    if answer.len() as u64 > limit as u64 {
        return Err("the answer is longer than its budget");
    }
    if answer.iter().any(|id| config.bias_of(*id) == PalwDecodeBiasV4::Ban) {
        return Err("the answer commits a lane logit_bias bans");
    }
    match decode_first_stop_v4(&config.stop_sequences, answer) {
        Some((end, index)) if end == answer.len() => {
            Ok(PalwFpDecodeStopV1 { executed: end as u32, reason: PalwFpDecodeStopReasonV1::StopSequence { index: index as u8 } })
        }
        Some(_) => Err("the answer continues past a completed stop sequence"),
        None if answer.len() as u64 == limit as u64 => {
            Ok(PalwFpDecodeStopV1 { executed: limit, reason: PalwFpDecodeStopReasonV1::Budget })
        }
        // Without a constraint the admitted set is the same at every position, so an early end
        // with no stop sequence is admissible only if that set is empty — and then no position
        // could have committed anything, which the first check has already refused.
        None if config.bans_cover_vocab(vocab) => Err("no lane is admissible, so no token could have been committed"),
        None => Err("the answer stops before its budget with no stop sequence completed"),
    }
}

#[cfg(test)]
mod processor_tests {
    use super::*;
    use crate::palw_decode_select_v2::PALW_DECODE_T_ONE;
    use crate::palw_step_refute::base0_decode_token_select_v1;

    /// The same deterministic row generator the v2 tests use — no dev-dependency.
    fn rows(seed: u64, count: usize, width: usize, spread: i32) -> Vec<Vec<i32>> {
        let mut x = seed | 1;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        (0..count).map(|_| (0..width).map(|_| ((next() % (2 * spread as u64 + 1)) as i64 - spread as i64) as i32).collect()).collect()
    }

    fn hot() -> PalwDecodeSamplingV2 {
        PalwDecodeSamplingV2 { seed: [7u8; 32], temperature_q: PALW_DECODE_T_ONE as u32 }
    }

    #[test]
    fn the_noop_selects_what_v3_selects_on_every_row() {
        // G7 at the processor: the no-op V4 is the V3 rule — greedy (Decision 11 at T = 0 is the
        // shipped argmax) and sampled alike — on random rows, with random committed prefixes.
        for (i, row) in rows(0x5eed, 400, 97, 1 << 26).into_iter().enumerate() {
            let generated: Vec<u32> = rows(i as u64 + 3, 1, i % 40, 48)[0].iter().map(|v| v.unsigned_abs() % 97).collect();
            for sampling in [PalwDecodeSamplingV2::GREEDY, hot()] {
                let v3 = decode_token_select_v2(&row, &sampling.seed, generated.len() as u32, sampling.temperature_q);
                let v4 = decode_select_v4(&DecodeConfigV4::NOOP, &sampling, &generated, &row, &|_| true);
                assert_eq!(v4, Some(v3), "row {i}");
            }
            assert_eq!(
                decode_select_v4(&DecodeConfigV4::NOOP, &PalwDecodeSamplingV2::GREEDY, &generated, &row, &|_| true),
                Some(base0_decode_token_select_v1(&row)),
                "row {i}: greedy no-op is the shipped argmax"
            );
        }
    }

    #[test]
    fn the_row_form_is_the_argmax_of_the_per_lane_form() {
        // I-2: whatever the row-wide merge does, it is the argmax of the per-lane key a court
        // recomputes from one tile. Swept over penalties, bias (with bans) and both temperatures.
        let ban = PALW_DECODE_V4_BIAS_BAN_Q;
        let configs = [
            DecodeConfigV4 { repeat_penalty_q: 98_304, penalty_window: 6, ..DecodeConfigV4::NOOP },
            DecodeConfigV4 {
                frequency_penalty_q: 1 << 23,
                presence_penalty_q: -(1 << 22),
                penalty_window: 16,
                ..DecodeConfigV4::NOOP
            },
            DecodeConfigV4 { logit_bias: vec![(0, ban), (3, 5 << 24), (17, -(7 << 24)), (40, ban)], ..DecodeConfigV4::NOOP },
            DecodeConfigV4 {
                repeat_penalty_q: 262_144,
                penalty_window: 256,
                frequency_penalty_q: 2 << 24,
                presence_penalty_q: 2 << 24,
                logit_bias: vec![(1, 100 << 24), (2, ban)],
                stop_sequences: vec![],
            },
        ];
        for (c, config) in configs.iter().enumerate() {
            config.validate_canonical().expect("the sweep uses canonical configs");
            for (i, row) in rows(0xC0FFEE + c as u64, 120, 48, 1 << 25).into_iter().enumerate() {
                let generated: Vec<u32> = rows(i as u64 + 11, 1, 30, 24)[0].iter().map(|v| v.unsigned_abs() % 48).collect();
                for sampling in [PalwDecodeSamplingV2::GREEDY, hot()] {
                    let picked = decode_select_v4(config, &sampling, &generated, &row, &|_| true);
                    let mut best: Option<(usize, i64)> = None;
                    for (lane, value) in row.iter().enumerate() {
                        if let Some(key) = decode_lane_key_v4(config, &sampling, &generated, *value, lane as u32) {
                            best = match best {
                                Some((bl, bk)) if !decode_lane_beats_v2(key, lane, bk, bl) => Some((bl, bk)),
                                _ => Some((lane, key)),
                            };
                        }
                    }
                    assert_eq!(picked, best.map(|(l, _)| l), "config {c}, row {i}");
                }
            }
        }
    }

    #[test]
    fn the_processor_order_is_the_rfcs() {
        // repeat before frequency: the repeat divides the RAW value, and the frequency subtracts
        // from what the repeat left. Lane 1 was generated twice; W covers both.
        let config = DecodeConfigV4 {
            repeat_penalty_q: 131_072, // 2.0
            penalty_window: 4,
            frequency_penalty_q: 1 << 24,
            presence_penalty_q: 1 << 23,
            logit_bias: vec![(1, 3 << 24)],
            stop_sequences: vec![],
        };
        let v = 10i32 << 24;
        // a = 10/2 = 5; b = 5 − 2·1 − 0.5 = 2.5; d = 2.5 + 3 = 5.5 (in Q24)
        assert_eq!(decode_lane_value_v4(&config, v, 1, 2), Some((11 << 24) / 2));
        // bias after the penalties, not before: (10 + 3)/2 − 2.5 = 4 would be the wrong order.
        assert_ne!(decode_lane_value_v4(&config, v, 1, 2), Some(4 << 24));
        // saturation last among the value steps: +100 on i32::MAX saturates, −∞ stops at MIN + 1.
        let big = DecodeConfigV4 { logit_bias: vec![(0, 100 << 24)], ..DecodeConfigV4::NOOP };
        assert_eq!(decode_lane_value_v4(&big, i32::MAX, 0, 0), Some(i32::MAX));
        let low = DecodeConfigV4 { logit_bias: vec![(0, -(99 << 24))], ..DecodeConfigV4::NOOP };
        assert_eq!(decode_lane_value_v4(&low, i32::MIN, 0, 0), Some(i32::MIN + 1));
        assert_eq!(decode_lane_value_v4(&DecodeConfigV4::NOOP, i32::MIN, 0, 0), Some(i32::MIN + 1));
        // the mask after the values: a banned lane with the highest value is never selected, and
        // the constraint composes with the bans.
        let ban = DecodeConfigV4 { logit_bias: vec![(2, PALW_DECODE_V4_BIAS_BAN_Q)], ..DecodeConfigV4::NOOP };
        let row = [1, 5, 9, 7];
        assert_eq!(decode_select_v4(&ban, &PalwDecodeSamplingV2::GREEDY, &[], &row, &|_| true), Some(3));
        assert_eq!(decode_select_v4(&ban, &PalwDecodeSamplingV2::GREEDY, &[], &row, &|lane| lane != 3), Some(1));
        assert_eq!(decode_select_v4(&ban, &PalwDecodeSamplingV2::GREEDY, &[], &row, &|lane| lane == 2), None, "A(t) empty");
        // ties to the lowest index, AFTER processing: the bias lifts lane 0 into the tie and wins it.
        assert_eq!(decode_select_v4(&DecodeConfigV4::NOOP, &PalwDecodeSamplingV2::GREEDY, &[], &[5, 7, 7], &|_| true), Some(1));
        let tie = DecodeConfigV4 { logit_bias: vec![(0, 2)], ..DecodeConfigV4::NOOP };
        assert_eq!(decode_select_v4(&tie, &PalwDecodeSamplingV2::GREEDY, &[], &[5, 7, 7], &|_| true), Some(0));
    }

    #[test]
    fn the_decoder_stops_at_the_first_stop_and_resumes_exactly() {
        let config = DecodeConfigV4 { stop_sequences: vec![vec![2, 3]], ..DecodeConfigV4::NOOP };
        // Rows whose argmax walks 1, 2, 3, 0 …
        let row_for = |lane: usize| {
            let mut r = vec![0i32; 4];
            r[lane] = 10;
            r
        };
        let mut decoder = PalwFpDecoderV1::v4(config.clone(), PalwDecodeSamplingV2::GREEDY, 8);
        let fed: Vec<u32> = [1usize, 2, 3, 0, 1].iter().map(|l| decoder.select(&row_for(*l))).collect();
        assert_eq!(fed, vec![1, 2, 3, 0, 1], "past the stop the loop is still fed");
        assert_eq!(decoder.generated(), &[1, 2, 3]);
        assert_eq!(
            decoder.stop(),
            Some(PalwFpDecodeStopV1 { executed: 3, reason: PalwFpDecodeStopReasonV1::StopSequence { index: 0 } })
        );
        // A replay from the committed prefix is the same decoder.
        let resumed = PalwFpDecoderV1::v4(config.clone(), PalwDecodeSamplingV2::GREEDY, 8).resumed(&[1, 2]);
        assert_eq!(resumed.generated(), &[1, 2]);
        assert_eq!(resumed.stop(), None);
        assert_eq!(
            PalwFpDecoderV1::v4(config.clone(), PalwDecodeSamplingV2::GREEDY, 8).resumed(&[1, 2, 3, 9]).generated(),
            &[1, 2, 3]
        );
        // The answer check agrees with the decoder, and refuses an answer that ran past its stop.
        assert_eq!(decode_answer_stop_v4(&config, 8, 4, &[1, 2, 3]), Ok(decoder.stop().unwrap()));
        assert!(decode_answer_stop_v4(&config, 8, 4, &[1, 2, 3, 0]).is_err());
        assert!(decode_answer_stop_v4(&config, 8, 4, &[1, 2]).is_err(), "short of the budget with no stop");
        assert_eq!(
            decode_answer_stop_v4(&config, 2, 4, &[1, 2]),
            Ok(PalwFpDecodeStopV1 { executed: 2, reason: PalwFpDecodeStopReasonV1::Budget })
        );
        // The budget stops the V3 rule and never anything earlier.
        let mut v3 = PalwFpDecoderV1::v3(PalwDecodeSamplingV2::GREEDY, 2);
        v3.select(&row_for(2));
        assert_eq!(v3.stop(), None);
        v3.select(&row_for(3));
        assert_eq!(v3.stop(), Some(PalwFpDecodeStopV1 { executed: 2, reason: PalwFpDecodeStopReasonV1::Budget }));
        // An empty admitted set stops before committing.
        let all_banned =
            DecodeConfigV4 { logit_bias: (0..4).map(|t| (t, PALW_DECODE_V4_BIAS_BAN_Q)).collect(), ..DecodeConfigV4::NOOP };
        let mut stuck = PalwFpDecoderV1::v4(all_banned.clone(), PalwDecodeSamplingV2::GREEDY, 8);
        stuck.select(&row_for(1));
        assert_eq!(stuck.stop(), Some(PalwFpDecodeStopV1 { executed: 0, reason: PalwFpDecodeStopReasonV1::NoAdmissibleLane }));
        assert!(stuck.generated().is_empty());
        assert!(all_banned.bans_cover_vocab(4));
    }

    #[test]
    fn a_penalty_moves_the_choice_off_a_repeated_lane() {
        // The point of the release, in one row: greedy would repeat lane 0; the penalties do not.
        let row = [10i32 << 24, 9 << 24, 1 << 24];
        let generated = [0u32, 0, 0];
        let greedy = decode_select_v4(&DecodeConfigV4::NOOP, &PalwDecodeSamplingV2::GREEDY, &generated, &row, &|_| true);
        assert_eq!(greedy, Some(0));
        let penalized = DecodeConfigV4 { repeat_penalty_q: 81_920, penalty_window: 8, ..DecodeConfigV4::NOOP };
        assert_eq!(decode_select_v4(&penalized, &PalwDecodeSamplingV2::GREEDY, &generated, &row, &|_| true), Some(1));
        let frequency = DecodeConfigV4 { frequency_penalty_q: 1 << 24, penalty_window: 8, ..DecodeConfigV4::NOOP };
        assert_eq!(decode_select_v4(&frequency, &PalwDecodeSamplingV2::GREEDY, &generated, &row, &|_| true), Some(1));
        // Outside the window the repetition is forgotten.
        let narrow = DecodeConfigV4 { frequency_penalty_q: 1 << 24, penalty_window: 1, ..DecodeConfigV4::NOOP };
        assert_eq!(decode_select_v4(&narrow, &PalwDecodeSamplingV2::GREEDY, &[0, 0, 1], &row, &|_| true), Some(0));
    }
}

// ---------------------------------------------------------------------------------------------
// Engines and seats: one driver for a producer's run, one scoped rule for a replay
// ---------------------------------------------------------------------------------------------

/// **Run a free-prompt job under its decoder, to exactly where the decoder stops** — the one
/// driver every free-prompt engine's producer calls (RFC-0001 §A.3 on the worker, G4).
///
/// `run(count, select, stream)` is ONE capture of the job at `count` decode tokens: the engine
/// builds its job context for `count` (a step leaf binds the context, and the context binds the
/// count, so hashing cannot begin before the count is fixed), feeds `select(row)`'s lane into its
/// next forward pass for each selecting row in order, and calls `stream(id)` for each id it feeds.
/// The driver runs it once at the job's budget; if the pipeline stopped earlier (a stop sequence
/// completed, or no lane was admissible) it runs it again at exactly that count — selection is a
/// pure function of the committed prefix, so the second run commits the same tokens, which is
/// checked. Only committed ids are streamed, once. A V3 job never stops before its budget and runs
/// exactly once, byte for byte as before. `committed_of` reads the ids a run committed.
///
/// Refused before anything runs: a V4 job whose `logit_bias` bans every lane of a `vocab`-wide row
/// (no position could commit a token). Refused after the first run: a job that stops before its
/// first token.
pub fn palw_fp_decode_run_v1<R>(
    job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
    vocab: u32,
    on_token: &mut dyn FnMut(u32),
    mut run: impl FnMut(u32, &mut dyn FnMut(&[i32]) -> u32, &mut dyn FnMut(u32)) -> Result<R, String>,
    committed_of: impl Fn(&R) -> &[u32],
) -> Result<(R, PalwFpDecodeStopV1), String> {
    if let Some(decode) = job.decode.as_ref().filter(|_| job.is_v4())
        && decode.bans_cover_vocab(vocab)
    {
        return Err("logit_bias bans every lane of this class's vocabulary: no position could commit a token".to_string());
    }
    let limit = job.decode_token_limit;
    let decoder = std::cell::RefCell::new(job.decoder_v1());
    let streamed = std::cell::Cell::new(0usize);
    let first = {
        let mut select = |row: &[i32]| decoder.borrow_mut().select(row);
        let mut stream = |id: u32| {
            if streamed.get() < decoder.borrow().generated().len() {
                streamed.set(streamed.get() + 1);
                on_token(id);
            }
        };
        run(limit, &mut select, &mut stream)?
    };
    let decoder = decoder.into_inner();
    let stop = decoder
        .stop()
        .unwrap_or(PalwFpDecodeStopV1 { executed: decoder.generated().len() as u32, reason: PalwFpDecodeStopReasonV1::Budget });
    if stop.executed >= limit {
        if committed_of(&first) != decoder.generated() {
            return Err("the engine fed ids the decoder did not commit".to_string());
        }
        return Ok((first, stop));
    }
    if stop.executed == 0 {
        return Err(
            "no lane is admissible at the first decode position: the job's logit_bias bans every lane it could commit".to_string()
        );
    }
    let again = std::cell::RefCell::new(job.decoder_v1());
    let second = {
        let mut select = |row: &[i32]| again.borrow_mut().select(row);
        run(stop.executed, &mut select, &mut |_| {})?
    };
    if committed_of(&second) != decoder.generated() {
        return Err(format!(
            "the run at the stop ({} tokens) did not commit the tokens the budget run committed — selection is not a function of the prefix",
            stop.executed
        ));
    }
    Ok((second, stop))
}

/// **The rule a seat's replay applies, scoped to one verification** — the claim's decode rule
/// and its committed answer, so the replay derives each selecting row's id exactly as the
/// producer's pipeline did.
///
/// A replay step selects from row `r` with `c_j` counted over the committed ids `0..r` — the
/// claim's own answer, which the seat authenticated against the claim before replaying (it is the
/// answer whose `output_root` the chain holds). An honest producer's committed id at row `r` is
/// then exactly what this selects, so the replayed rows are the committed ones; a producer that
/// committed any other id diverges at that row and the seat files nothing.
///
/// **The executor's own replays are FORCED instead** ([`Self::forced`]): an executor re-deriving the
/// leaves of its own retained capture (an interval opening, a segment checkpoint, a dense capture for
/// the court) already holds the committed ids, and replays them rather than re-selecting — which is
/// byte-identical for every V3 claim (its committed ids ARE the shipped argmax) and is the only
/// replay that reproduces a V4 capture without the job's decode config at hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFpReplayRuleV1 {
    kind: PalwFpReplayKindV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PalwFpReplayKindV1 {
    /// A seat's check: the claim's pipeline over its committed answer.
    Pipeline { config: DecodeConfigV4, sampling: PalwDecodeSamplingV2, committed: Vec<u32> },
    /// The executor's own replay: the committed ids, fed back.
    Forced(Vec<u32>),
}

impl PalwFpReplayRuleV1 {
    /// The rule of a V4 job over its committed answer; `None` for a V3 job, whose replay keeps the
    /// shipped rule (the V3 verifier) byte for byte.
    pub fn of_job(job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3, committed: &[u32]) -> Option<Self> {
        let config = job.decode.as_ref().filter(|_| job.is_v4())?.clone();
        Some(Self { kind: PalwFpReplayKindV1::Pipeline { config, sampling: job.sampling_v2(), committed: committed.to_vec() } })
    }

    /// The executor's replay of its own retained answer: row `r` feeds committed id `r`.
    pub fn forced(committed: &[u32]) -> Self {
        Self { kind: PalwFpReplayKindV1::Forced(committed.to_vec()) }
    }

    /// The id row `row_index` selects: §A.3 over the committed prefix `0..row_index` (a seat), or
    /// the committed id itself (the executor).
    pub fn select(&self, row: &[i32], row_index: u32) -> u32 {
        match &self.kind {
            PalwFpReplayKindV1::Forced(committed) => committed
                .get(row_index as usize)
                .copied()
                .unwrap_or_else(|| crate::palw_step_refute::base0_decode_token_select_v1(row) as u32),
            PalwFpReplayKindV1::Pipeline { config, sampling, committed } => {
                let before = &committed[..(row_index as usize).min(committed.len())];
                match decode_select_v4(config, sampling, before, row, &|_| true) {
                    Some(lane) => lane as u32,
                    // An empty admitted set commits nothing; a replay step never reaches one on an
                    // honest claim (the producer stopped there), and the raw rule keeps this total.
                    None => decode_token_select_v2(row, &sampling.seed, row_index, sampling.temperature_q) as u32,
                }
            }
        }
    }
}

thread_local! {
    static PALW_FP_REPLAY_RULE: std::cell::RefCell<Option<std::rc::Rc<PalwFpReplayRuleV1>>> =
        const { std::cell::RefCell::new(None) };
}

/// **Run `f` with `rule` as this thread's replay rule** — the scope a seat opens around one
/// verification of one claim (`PalwExecutionBackendV1::verify_fp_interval_opening_under_job_v1`).
/// Restored on exit, panics included, so one claim's rule can never leak into the next check.
/// `None` is the V3 verifier.
pub fn palw_fp_with_replay_rule_v1<R>(rule: Option<PalwFpReplayRuleV1>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<std::rc::Rc<PalwFpReplayRuleV1>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            PALW_FP_REPLAY_RULE.with(|slot| *slot.borrow_mut() = previous);
        }
    }
    let previous = PALW_FP_REPLAY_RULE.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), rule.map(std::rc::Rc::new)));
    let _restore = Restore(previous);
    f()
}

/// **The id a replay derives from selecting row `row_index`** — the scoped V4 rule when a seat
/// opened one ([`palw_fp_with_replay_rule_v1`]), otherwise the shipped rule
/// (`base0_decode_token_select_v1`), which is what every V3 claim — and every attempt — commits.
pub fn palw_fp_replay_select_v1(row: &[i32], row_index: u32) -> u32 {
    let scoped = PALW_FP_REPLAY_RULE.with(|slot| slot.borrow().clone());
    match scoped {
        Some(rule) => rule.select(row, row_index),
        None => crate::palw_step_refute::base0_decode_token_select_v1(row) as u32,
    }
}

#[cfg(test)]
mod engine_driver_tests {
    use super::*;
    use crate::palw_freeprompt_v3::{PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3};
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn job(limit: u32) -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: crate::Hash64::from_u64_word(1),
            class_id: crate::Hash64::from_u64_word(2),
            executor_bond: TransactionOutpoint { transaction_id: TransactionId::from_u64_word(3), index: 0 },
            executor_pubkey: vec![4],
            operator_id: crate::Hash64::from_u64_word(5),
            anchor_block: crate::Hash64::from_u64_word(6),
            anchor_daa: 7,
            job_nonce: [8; 32],
            tokenizer_id: crate::Hash64::from_u64_word(9),
            prompt_token_ids_hash: crate::Hash64::from_u64_word(10),
            prompt_tokens: 1,
            decode_token_limit: limit,
            max_context_tokens: 64,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: [0; 32],
            temperature_q: 0,
            decode: None,
            images: None,
        }
    }

    /// A toy engine: the logits row after feeding token `t` peaks at `(t + 1) % 4`, so the greedy
    /// answer walks 1, 2, 3, 0, 1, … — `runs` counts captures.
    fn engine(count: u32, select: &mut dyn FnMut(&[i32]) -> u32, stream: &mut dyn FnMut(u32), runs: &mut u32) -> Vec<u32> {
        *runs += 1;
        let mut fed = Vec::new();
        let mut last = 0u32;
        for _ in 0..count {
            let mut row = vec![0i32; 4];
            row[((last + 1) % 4) as usize] = 10;
            let id = select(&row);
            fed.push(id);
            stream(id);
            last = id;
        }
        fed
    }

    #[test]
    fn a_v3_job_runs_once_to_its_budget() {
        let (mut runs, mut streamed) = (0, Vec::new());
        let (fed, stop) =
            palw_fp_decode_run_v1(&job(6), 4, &mut |id| streamed.push(id), |n, s, t| Ok(engine(n, s, t, &mut runs)), |r| r).unwrap();
        assert_eq!((fed.clone(), runs), (vec![1, 2, 3, 0, 1, 2], 1));
        assert_eq!(streamed, fed);
        assert_eq!(stop, PalwFpDecodeStopV1 { executed: 6, reason: PalwFpDecodeStopReasonV1::Budget });
    }

    #[test]
    fn a_stop_sequence_reruns_at_the_stop_and_streams_each_committed_id_once() {
        let v4 = job(8).into_v4(DecodeConfigV4 { stop_sequences: vec![vec![2, 3]], ..DecodeConfigV4::NOOP });
        let (mut runs, mut streamed) = (0, Vec::new());
        let (fed, stop) =
            palw_fp_decode_run_v1(&v4, 4, &mut |id| streamed.push(id), |n, s, t| Ok(engine(n, s, t, &mut runs)), |r| r).unwrap();
        assert_eq!(fed, vec![1, 2, 3], "the capture is at the stop");
        assert_eq!(runs, 2, "the budget run, then the run at the stop");
        assert_eq!(streamed, vec![1, 2, 3], "only committed ids, once");
        assert_eq!(stop, PalwFpDecodeStopV1 { executed: 3, reason: PalwFpDecodeStopReasonV1::StopSequence { index: 0 } });
        // A no-op V4 is the V3 run, once.
        let (mut runs, mut streamed) = (0, Vec::new());
        let (noop, _) = palw_fp_decode_run_v1(
            &job(6).into_v4(DecodeConfigV4::NOOP),
            4,
            &mut |id| streamed.push(id),
            |n, s, t| Ok(engine(n, s, t, &mut runs)),
            |r| r,
        )
        .unwrap();
        assert_eq!((noop, runs), (vec![1, 2, 3, 0, 1, 2], 1));
    }

    #[test]
    fn a_job_that_can_commit_nothing_is_refused() {
        let all = job(4)
            .into_v4(DecodeConfigV4 { logit_bias: (0..4).map(|t| (t, PALW_DECODE_V4_BIAS_BAN_Q)).collect(), ..DecodeConfigV4::NOOP });
        let mut runs = 0;
        let err = palw_fp_decode_run_v1(&all, 4, &mut |_| {}, |n, s, t| Ok(engine(n, s, t, &mut runs)), |r| r).unwrap_err();
        assert!(err.contains("bans every lane"), "{err}");
        assert_eq!(runs, 0, "refused before anything runs");
    }

    #[test]
    fn the_scoped_replay_rule_is_the_producers_and_does_not_leak() {
        let v4 = job(8).into_v4(DecodeConfigV4 { repeat_penalty_q: 262_144, penalty_window: 4, ..DecodeConfigV4::NOOP });
        let row = [10i32 << 24, 9 << 24, 0, 0];
        assert_eq!(palw_fp_replay_select_v1(&row, 1), 0, "unscoped: the shipped rule");
        let rule = PalwFpReplayRuleV1::of_job(&v4, &[0, 1]);
        let inside = palw_fp_with_replay_rule_v1(rule, || palw_fp_replay_select_v1(&row, 1));
        assert_eq!(inside, 1, "scoped: lane 0 was committed at row 0, so the repeat penalty moves row 1 off it");
        assert_eq!(palw_fp_replay_select_v1(&row, 1), 0, "and the scope is gone afterwards");
        assert!(PalwFpReplayRuleV1::of_job(&job(8), &[0]).is_none(), "a V3 job keeps the V3 verifier");
        // Restored across a panic too.
        let caught = std::panic::catch_unwind(|| {
            palw_fp_with_replay_rule_v1(PalwFpReplayRuleV1::of_job(&v4, &[0]), || panic!("a replay that panics"))
        });
        assert!(caught.is_err());
        assert_eq!(palw_fp_replay_select_v1(&row, 1), 0);
    }
}
