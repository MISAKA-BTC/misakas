//! **Effective false-accept accounting** (the user's ruling of 2026-10-08: an EFFECTIVE 128-bit bound, counting retries, grinding,
//! multiple relations and adaptive attacks — never the per-check number alone).
//!
//! ```text
//! eff = min_i (r · s_i)  −  ⌈log2 m⌉  −  ⌈log2 (R + 1)⌉  −  β · ⌈log2 G⌉  −  ⌈log2 Q⌉
//! ```
//!
//! * `s_i` — relation family `i`'s soundness for ONE repetition, `−log2 ε_i` in millibits (or [`RelationSoundnessV1::Complete`]:
//!   the family's whole domain is checked, `ε_i = 0`);
//! * `r` — the policy's `repetition_count` (independent repetitions multiply the per-repetition bits);
//! * `m` — the sampled families: an adversary may lie in whichever is weakest, and a statement is false if any family is, so the
//!   union bound over families is charged;
//! * `R` — the policy's `retry_limit`: `R + 1` attempts, each a fresh beacon and seed;
//! * `G` — the grinding choices per beacon (beacon outputs an adversary can select among: output selection, withholding, timing,
//!   fork choice, concentration, last-contributor grinding — `docs/design/palw/opv-beacon-bootstrap.md` §6), `β` the beacons one
//!   attempt draws (1 non-interactive; the rounds of a staged beacon);
//! * `Q` — adaptive statements: the subjects an adversary may commit against the same policy over its life (Sybil classes of one
//!   model, claims of one class).
//!
//! The terms multiply the adversary's tries (`P(false accept) ≤ m · (R+1) · G^β · Q · 2^(−min r·s_i)`), so their logarithms
//! subtract. Every logarithm is rounded UP and every factor charged separately (the sum of ceilings is at least the ceiling of the
//! sum): the result never overstates the bound. Integer arithmetic only, so every machine and every release computes the same number.
//! If every family is complete, the bound is [`EffectiveBitsV1::Complete`] whatever the other terms: no number of tries makes an
//! enumeration miss what it enumerates.

use borsh::{BorshDeserialize, BorshSerialize};

/// **The floor an approved policy's target must reach**, in effective bits.
pub const APPROVAL_MIN_TARGET_BITS_V1: u16 = 128;

/// Millibits in a bit.
pub const MILLIBITS_PER_BIT_V1: u64 = 1_000;

/// One relation family's soundness for ONE repetition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum RelationSoundnessV1 {
    /// `ε = 0`: every input of the family's (finite) domain is checked.
    Complete = 0,
    /// `−log2 ε` of one repetition, in millibits.
    MilliBits(u64) = 1,
}

/// What the accounting reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveSoundnessInputV1 {
    pub relations: Vec<RelationSoundnessV1>,
    pub repetition_count: u32,
    pub retry_limit: u32,
    /// `G ≥ 1` (1: the adversary has no choice of beacon).
    pub grinding_choices_per_beacon: u128,
    /// `β ≥ 1`.
    pub beacons_per_attempt: u32,
    /// `Q ≥ 1`.
    pub adaptive_queries: u128,
}

/// The effective bound. `Bits < Complete` in the order (a complete check meets every target).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectiveBitsV1 {
    Bits(u16),
    Complete,
}

impl EffectiveBitsV1 {
    /// Whether the bound reaches `target` effective bits.
    pub fn meets(self, target: u16) -> bool {
        match self {
            Self::Complete => true,
            Self::Bits(b) => b >= target,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SoundnessRefusalV1 {
    #[error("no relation family is stated")]
    NoRelation,
    #[error("zero repetitions")]
    ZeroRepetitions,
    #[error("zero grinding choices (a beacon always has at least the one output it locks)")]
    ZeroGrindingChoices,
    #[error("zero beacons per attempt")]
    ZeroBeacons,
    #[error("zero adaptive queries (the adversary always has at least its one statement)")]
    ZeroQueries,
}

/// `⌈log2 x⌉` for `x ≥ 1` (`0` for 1; `128` for `u128::MAX`); `0` for 0 as well (callers refuse 0 first).
pub fn ceil_log2_v1(x: u128) -> u64 {
    if x <= 1 { 0 } else { 128 - (x - 1).leading_zeros() as u64 }
}

/// **The effective false-accept bound, in bits** (module doc).
pub fn effective_false_accept_bits_v1(input: &EffectiveSoundnessInputV1) -> Result<EffectiveBitsV1, SoundnessRefusalV1> {
    use SoundnessRefusalV1 as R;
    if input.relations.is_empty() {
        return Err(R::NoRelation);
    }
    if input.repetition_count == 0 {
        return Err(R::ZeroRepetitions);
    }
    if input.grinding_choices_per_beacon == 0 {
        return Err(R::ZeroGrindingChoices);
    }
    if input.beacons_per_attempt == 0 {
        return Err(R::ZeroBeacons);
    }
    if input.adaptive_queries == 0 {
        return Err(R::ZeroQueries);
    }
    // r · s_i for every sampled family, in millibits (u64 × u32 fits u128 exactly).
    let sampled: Vec<u128> = input
        .relations
        .iter()
        .filter_map(|r| match r {
            RelationSoundnessV1::Complete => None,
            RelationSoundnessV1::MilliBits(s) => Some(*s as u128 * input.repetition_count as u128),
        })
        .collect();
    let Some(weakest) = sampled.iter().copied().min() else { return Ok(EffectiveBitsV1::Complete) };
    let loss_bits: u128 = ceil_log2_v1(sampled.len() as u128) as u128
        + ceil_log2_v1(input.retry_limit as u128 + 1) as u128
        + input.beacons_per_attempt as u128 * ceil_log2_v1(input.grinding_choices_per_beacon) as u128
        + ceil_log2_v1(input.adaptive_queries) as u128;
    let effective_millibits = weakest.saturating_sub(loss_bits * MILLIBITS_PER_BIT_V1 as u128);
    Ok(EffectiveBitsV1::Bits((effective_millibits / MILLIBITS_PER_BIT_V1 as u128).min(u16::MAX as u128) as u16))
}

/// The per-repetition millibits of a family whose fault shows on at least `fault_ppm` of its draws, with `draws` independent
/// uniform draws per repetition: `−log2 ε ≥ n · f · log2 e` (`log2 e > 1.4426`), the conformance scope's own model.
pub fn sampled_family_millibits_v1(draws: u64, fault_ppm: u32) -> u64 {
    // n · (ppm / 10^6) · 1.4426 bits = n · ppm · 14,426 / 10^7 millibits (the scope's `derived_epsilon_bits` × 1,000, floored).
    (draws as u128 * fault_ppm as u128 * 14_426 / 10_000_000).min(u64::MAX as u128) as u64
}

/// **The grinding choices a beacon of `k` sources offers when every work that can enter its window is one of at most `live_cap`
/// concurrently live works** (the OPV ledger-wide live-claim cap, with `beacon_window_slots ≤` the OPV window + liability), the
/// sources' contents are fixed when they are committed, and `fork_alternatives` branches can each lock their own beacon:
/// `P(live_cap, k) · F` ordered source lists. `k = 0` (no beacon) is 1. Saturating.
///
/// This bounds output selection, withholding before the lock, Final timing and concentration. It does NOT bound a last contributor
/// who grinds a free input (a job nonce) offline before committing: that adversary's choices are its offline work, which the
/// accounting must state instead (`docs/design/palw/opv-beacon-bootstrap.md` §6.2).
pub fn beacon_grinding_choices_bound_v1(k: u32, live_cap: u32, fork_alternatives: u128) -> u128 {
    let take = k.min(live_cap);
    let mut lists: u128 = 1;
    for i in 0..take {
        lists = lists.saturating_mul((live_cap - i) as u128);
    }
    lists.max(1).saturating_mul(fork_alternatives.max(1))
}
