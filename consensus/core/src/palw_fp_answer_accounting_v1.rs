//! **A decoded answer is not paid twice** (compute-claim audit CX-01 / CX-02, lane compute-exec, 2026-10-22; ADR-0205 draft,
//! RFC-0038 draft) — `Params::palw_fp_answer_accounting_v1`, dormant.
//!
//! # The defect
//!
//! A deterministic causal model makes the computation at every position a function of the tokens at and before it. A free-prompt
//! run of `P` prompt ids that decoded `D` tokens touches `P + D − 1` positions. ADR-0145 §6 credits a claim only for positions past
//! the longest prefix of its PROMPT that a paid claim of the class already committed — but the chain records PROMPTS
//! (`fp_claim_prompts`), never the answers the paid claims decoded. So:
//!
//! * CX-01 — C1 = (`P ‖ g`, decode `d`), where `g` is C0's own answer, re-buys C0's decoded positions as "prefill" (live on
//!   testnet-12: `palw_fp_derived_work` at genesis, decode rules dormant), and past the decode rules C1 = (`P ‖ g[..k]`, decode
//!   `D − k`) re-buys C0's decode positions on every rung — `Σ (D − k) ≈ D²/2` credited for `P + D` computed;
//! * CX-02 — the same `(P, D)` under a second bond (`fp_work_id_v1` is per bond) is credited its whole decode again; the leaves are
//!   bond-independent values the producer re-hashes under the second bond's context, at hashing cost.
//!
//! Every such claim is an HONEST execution: each seat's whole-job replay (SEAT-R) reproduces it, and no court can convict it. The
//! price is the only place it can be refused.
//!
//! # The rule (past the fence)
//!
//! 1. A compute-priced commitment carries its answer ids (the `FPA1` trailer, [`palw_fp_answer_envelope_encode_v1`]); the fold
//!    refuses one without them, or with a count other than `decode_tokens_executed`.
//! 2. The fold records `prompt ‖ answer` as a paid sequence of the class (a second `fp_claim_prompts` row under
//!    [`palw_fp_answer_row_key_v1`]), with the same epoch-aligned expiry as the prompt row.
//! 3. A claim is credited only for the positions past `c`, the longest run of positions some unexpired paid row of the class
//!    already COMPUTED with the same tokens ([`palw_fp_answer_covered_positions_v1`]); `c ≥ P + D − 1` is a claim with nothing new
//!    and is refused (`ZeroQuanta`).
//!
//! What binds the carried answer to the run is the claim's own `output_root`: a seat's replay names the run's answer, and the seat
//! refuses a licence to a claim whose carried answer does not recompute that root under the claim's context (arming prerequisite —
//! see [`Params::validate_palw_fp_answer_accounting_v1`]). An answer that lies is therefore a claim that never licenses, so a lie
//! cannot shrink the coverage a later claim meets, and coverage an attacker plants can only match a sequence it actually computed.
mod binding;
pub use binding::*;
mod ingress;
pub use ingress::*;

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// The leading magic of the answer carriage: `FPA1 ‖ borsh(Vec<u8> inner payload) ‖ borsh(Vec<u32> answer)`.
pub const PALW_FP_ANSWER_ENVELOPE_MAGIC_V1: &[u8; 4] = b"FPA1";

/// Keyed-BLAKE2b-512 domain of [`palw_fp_answer_row_key_v1`].
pub const PALW_FP_ANSWER_ROW_KEY_DOMAIN_V1: &[u8] = b"misaka-palw/fp/answer-row-key/v1";

/// The refusal text of a compute-priced commitment that carries no answer (or one of the wrong length) past the fence.
pub const PALW_FP_ANSWER_NOT_ACCOUNTABLE_V1: &str =
    "past palw_fp_answer_accounting_v1 a free-prompt commitment carries its decoded answer ids, one per executed decode token";

/// **The key a claim's `prompt ‖ answer` row is filed under** in `fp_claim_prompts`, beside the prompt row (keyed by the claim id
/// itself, which the TIR public frame looks up by name). A distinct key, so no reader of the prompt row ever meets the sequence.
pub fn palw_fp_answer_row_key_v1(claim: &Hash64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_FP_ANSWER_ROW_KEY_DOMAIN_V1).to_state();
    state.update(claim.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **How many leading positions of a run some paid row of the class already computed with the same tokens.**
///
/// `rows` are `(ids, prompt_tokens)` of every unexpired paid row of the class. A prompt-only row (`ids.len() == prompt_tokens`)
/// computed every one of its positions; a `prompt ‖ answer` row computed all but the last answer token's
/// (`ids.len() − 1`). A run of `prompt ‖ answer` computes positions `0 .. prompt + answer − 1`, and position `t` is the same
/// computation as a paid row's position `t` exactly when the two sequences agree on `0 ..= t`.
pub fn palw_fp_answer_covered_positions_v1<'a>(
    rows: impl IntoIterator<Item = (&'a [u32], u32)>,
    prompt: &[u32],
    answer: &[u32],
) -> u32 {
    let sequence = || prompt.iter().chain(answer.iter());
    let mut best = 0usize;
    for (ids, prompt_tokens) in rows {
        let computed = if ids.len() > prompt_tokens as usize { ids.len() - 1 } else { ids.len() };
        let shared = ids.iter().zip(sequence()).take_while(|(a, b)| a == b).count();
        best = best.max(shared.min(computed));
    }
    best.min(u32::MAX as usize) as u32
}

/// **The answer carriage**: `FPA1 ‖ borsh(inner) ‖ borsh(answer)`, where `inner` is the commitment transaction's payload as it
/// would ride without an answer (a plain FP payload, or a `TFP1` / `HFP1` / `TFO1` envelope). A leading magic, as every FP envelope has, so
/// no plain payload (whose first two bytes are its version) and no other envelope is ever read as one.
pub fn palw_fp_answer_envelope_encode_v1(inner: &[u8], answer: &[u32]) -> Vec<u8> {
    let mut out = PALW_FP_ANSWER_ENVELOPE_MAGIC_V1.to_vec();
    borsh::BorshSerialize::serialize(&inner.to_vec(), &mut out).expect("a Vec writes into a Vec");
    borsh::BorshSerialize::serialize(&answer.to_vec(), &mut out).expect("a Vec writes into a Vec");
    out
}

/// Decode [`palw_fp_answer_envelope_encode_v1`]'s bytes, strictly: the magic, a bounded inner payload that is not itself an
/// answer envelope, an answer of at most `max_answer` ids, and nothing after it. `None` for every other payload.
pub fn palw_fp_answer_envelope_decode_v1(payload: &[u8], max_answer: usize) -> Option<(Vec<u8>, Vec<u32>)> {
    let view = palw_fp_answer_envelope_view_v1(payload, max_answer)?;
    Some((view.inner.to_vec(), view.answer_ids_v1().collect()))
}

impl Params {
    /// `palw_fp_answer_accounting_v1`'s height, where it is armed on a ConsensusV2 network (`None` everywhere else).
    pub fn palw_fp_answer_accounting_v1_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_answer_accounting_v1.filter(|f| *f != ForkActivation::never())) {
            (PalwConsensusMode::ConsensusV2(_), Some(at)) => Some(at),
            _ => None,
        }
    }

    /// **`palw_fp_answer_accounting_v1` is refused when armed** — on a network that is not ConsensusV2, without
    /// `palw_fp_derived_work` and `palw_canonical_work` in force at or below it (the rule re-prices the derived compute), and,
    /// until its arming record is pinned, at all. The node's bounded accounting-envelope decoder, native doors, transaction-local
    /// extraction and retained answer handoff to rehearsal/canonical extras are connected. The seat's carried-answer-vs-output-root
    /// check and producer emission remain arming prerequisites recorded in ADR-0205 (draft); framing or a local replay does not
    /// replace them.
    pub fn validate_palw_fp_answer_accounting_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_fp_answer_accounting_v1.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_fp_answer_accounting_v1 is armed on a network that is not ConsensusV2"));
        }
        let at_or_below =
            |other: Option<ForkActivation>| other.is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= fence.daa_score());
        if !at_or_below(self.palw_fp_derived_work) || !at_or_below(self.palw_canonical_work) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_answer_accounting_v1 needs palw_fp_derived_work and palw_canonical_work in force at or below it",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_fp_answer_accounting_v1 cannot be armed: its arming record (FPA1 node carriage, the seat's answer check, producer \
             emission; ADR-0205) is not pinned",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_is_the_computed_shared_run() {
        let prompt: Vec<u32> = (0..8).collect();
        let answer: Vec<u32> = (100..124).collect();
        let paid: Vec<u32> = prompt.iter().chain(answer.iter()).copied().collect();
        // A prompt-only row computed all of its positions.
        assert_eq!(palw_fp_answer_covered_positions_v1([(&prompt[..], 8)], &paid[..20], &[]), 8);
        // A sequence row computed all but its last answer token's position.
        assert_eq!(palw_fp_answer_covered_positions_v1([(&paid[..], 8)], &paid, &[7, 7]), 31);
        assert_eq!(palw_fp_answer_covered_positions_v1([(&paid[..], 8)], &prompt, &answer), 31);
        // A continuation that diverges at position 12 shares exactly 12.
        let mut other = paid.clone();
        other[12] = 9_999;
        assert_eq!(palw_fp_answer_covered_positions_v1([(&paid[..], 8)], &other, &[1]), 12);
        // No rows, no coverage.
        assert_eq!(palw_fp_answer_covered_positions_v1(std::iter::empty(), &prompt, &answer), 0);
    }

    #[test]
    fn the_envelope_round_trips_and_refuses_every_other_shape() {
        let inner = vec![3u8, 0, 1, 2, 3];
        let answer = vec![5u32, 6, 7];
        let bytes = palw_fp_answer_envelope_encode_v1(&inner, &answer);
        assert_eq!(palw_fp_answer_envelope_decode_v1(&bytes, 8), Some((inner.clone(), answer.clone())));
        assert_eq!(palw_fp_answer_envelope_decode_v1(&bytes, 2), None, "over the bound");
        assert_eq!(palw_fp_answer_envelope_decode_v1(&inner, 8), None, "no magic");
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(palw_fp_answer_envelope_decode_v1(&trailing, 8), None, "trailing bytes");
        let nested = palw_fp_answer_envelope_encode_v1(&bytes, &answer);
        assert_eq!(palw_fp_answer_envelope_decode_v1(&nested, 8), None, "an answer envelope inside an answer envelope");
        assert_ne!(palw_fp_answer_row_key_v1(&Hash64::from_u64_word(1)), Hash64::from_u64_word(1));
    }

    /// The fence's height reaches the printed fingerprint and the schedule id and never the identity (the fork-id rule every PALW
    /// fence keeps), it is on `palw_fences_v1`, testnet-12 ships it `None`, and arming it is refused until its record is pinned.
    #[test]
    fn the_fence_is_scheduled_like_every_palw_fence_and_refused_when_armed() {
        let base = crate::config::params::palw_rc_shipped_params();
        assert!(base.palw_fences_v1().iter().any(|(name, _)| *name == "palw_fp_answer_accounting_v1"));
        let mut early = base.clone();
        early.palw_fp_answer_accounting_v1 = Some(ForkActivation::new(9_000_000));
        let mut late = base.clone();
        late.palw_fp_answer_accounting_v1 = Some(ForkActivation::new(9_000_001));
        assert_ne!(early.consensus_params_id(), late.consensus_params_id());
        assert_ne!(early.consensus_schedule_id(), late.consensus_schedule_id());
        assert_eq!(early.consensus_identity_id(), late.consensus_identity_id());
        assert_ne!(base.consensus_params_id(), early.consensus_params_id(), "Some-only: an armed height moves the id");
        let shipped = crate::config::params::palw_t12_shipped_params();
        assert_eq!(shipped.palw_fp_answer_accounting_v1, None, "dormant on the shipped testnet-12 ruleset");
        let mut armed = shipped.clone();
        armed.palw_fp_answer_accounting_v1 = Some(ForkActivation::new(16_000));
        assert!(armed.validate_palw_fp_answer_accounting_v1().is_err(), "refused until its arming record is pinned");
        assert!(shipped.validate_palw_fp_answer_accounting_v1().is_ok());
    }
}
