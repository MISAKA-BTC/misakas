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

