//! **ADR-0152 v3.1 §3.11 (DA-1…DA-9): the data-availability court, redesigned** (F3, D3; M3).
//!
//! ADR-0062's court (and ADR-0103 Decision 4's held twin) put a claim under ONE session at a time:
//! the claim's phase became `DefaultDisputed`, a second accuser had nowhere to go, the session named
//! one unit the accuser chose, and nothing could be asked after the licence or after `Final`. Past
//! `Params::palw_rcore_plus` the court is this module's instead:
//!
//! * **sessions live in side maps** keyed `(claim, accuser)` — one open session per accuser per claim,
//!   many accusers per claim, the claim's phase never touched (DA-1, DA-2);
//! * **a session names one unit and the fold draws up to three more** inside the committed run, seeded
//!   by the block that accepts it, so the accuser cannot choose them and the producer cannot answer
//!   only the unit it was asked for (DA-3);
//! * **any bond with a live lock on the claim may answer** (X7), with `MaterialDisclosedV2` (tag 55),
//!   and every accepted answer answers every session that demands the unit (DA-4);
//! * **only a seat session pauses a pre-`Final` claim**, and the pause is credited back exactly (DA-5);
//! * **a refuted session costs `r × S_P(stage)`**, held until the claim resolves and refunded if the
//!   claim is convicted (DA-6); a default is the producer's S1/S3 and the covering signers' S4 (DA-7).
//!
//! This module holds the court's TYPES, constants, domains and pure functions — what the fold, the
//! acceptance layer and Phase 2's filers must compute identically. The fold's writers live in
//! `palw_state_v2` beside every other writer, and are dormant wherever `palw_rcore_plus` is.

use crate::Hash64;
use crate::palw_held_da_v1::{PalwHeldDisclosureCarriageV1, PalwHeldMissingV1};
use crate::palw_state_v2::PalwBondKeyV2;
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------------------------
// Constants (ADR §6 row 30)
// ---------------------------------------------------------------------------------------------

/// DA-3: the units the fold draws beside the named one, at most.
pub const PALW_DA_DRAWN_UNITS_V1: usize = 3;
/// DA-8: non-seat sessions open on one claim at once, at most.
pub const PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1: u8 = 3;
/// DA-8: non-seat sessions opened on one claim over its life, at most (`opened_non_seat_total`).
pub const PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1: u16 = 16;
/// DA-8: sessions one seat of the claim's current panel opens on the claim over its life, at most.
/// Seats are exempt from the lifetime cap above; this is their own budget.
pub const PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1: u8 = 4;
/// **Have seats landed automatic DA answering?** — `true` since Phase 2's P2-7. DA-7's S4 on
/// covering signers and N9's `ProducerWithholding` against them are fair only because a covering
/// signer can answer the demanded unit itself with the material it retained (DA-4, X7: any bond
/// with a live lock on the claim may disclose). kaspad's seats now do: every tick a seat reads
/// `palw_producer_v2::palw_disclosure_duties_v1` — the units of every open session on a claim where
/// its live lock's mask covers them, by the fold's own covering predicate — and answers each with a
/// `MaterialDisclosedV2` built by [`palw_da_answer_object_v1`] from the material it kept while the
/// lock lives (`palw_disclosure_duties_v1`'s `retain` pins it). So both are ARMED past
/// `palw_rcore_plus` ([`crate::palw_state_v2::palw_da_signer_liability_armed_v1`] reads this const).
///
/// **Whom it arms, and the residual (Phase 2 plan §5.6, X7; named).** Only a FULL mask covers a unit
/// ([`palw_da_unit_covered_by_v1`], C7): a partial seat — interval, segment or resume signer — is
/// never charged for what it never held, held units included (the audit's M3 deviation 5, decided
/// 2026-09-24: placing a held unit in a segment needs a v22 step-leaf count, and partial seats are
/// not bound at launch). So the plan's "free-prompt interval signers hold nothing to disclose"
/// charges nobody, and X7's liability is the full mask's — the full seat, a V2 `Valid`. Such a
/// signer answers from its verified copy, or from a capture re-made by replaying the claim's job
/// and checked against its roots: an attempt claim's from its block (chain data), a free-prompt
/// claim's from the job payload the seat kept when it licensed (a job-only `FPM1` licence keeps no
/// capture, so kaspad re-makes it on demand). The residual: a full-mask signer of a free-prompt claim
/// that no longer holds that job — lost to its disk, or taken by the retention janitor's space rule,
/// which drops such copies last — cannot answer, and is charged S4 for a default it could not prevent.
///
/// A consensus constant, not a switch: the fold reads it through
/// `PalwTransitionExtrasV1::seat_da_answer_landed`, which the processor sets to exactly this value,
/// and which only tests set otherwise (M3's `landed` twins test both sides).
pub const PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1: bool = true;
/// DA-6: `r`, the refuted-session cost as a fraction of the stage's reward base, in basis points —
/// the reporter reward's `r` (R-1, `PALW_RCORE_REPORTER_REWARD_BPS_V1`), so the refuted cost never
/// exceeds the reward a correct accusation earns.
pub const PALW_DA_REFUTED_COST_BPS_V1: u128 = crate::palw_state_v2::PALW_RCORE_REPORTER_REWARD_BPS_V1 as u128;

// ---------------------------------------------------------------------------------------------
// Domains and the one new ML-DSA-87 context (ADR §6 row 31; COMPLETE_V5's second addition)
// ---------------------------------------------------------------------------------------------

/// DA-3: the draw's seed, `H(domain ‖ ctx.block ‖ claim_id ‖ accuser)`.
pub const PALW_DA_DRAW_DOMAIN_V1: &[u8] = b"misaka-palw/da-draw/v1";
/// DA-7 / V-2b: the evidence half of a `DaDefault` record's key, `H(domain ‖ claim_id)`.
pub const PALW_DA_OFFENCE_KEY_DOMAIN_V1: &[u8] = b"misaka-palw/da-offence-key/v1";
/// DA-4: the digest an answer is signed over.
pub const PALW_DA_ANSWER_DIGEST_DOMAIN_V1: &[u8] = b"misaka-palw/da-disclosure-v4/answer-digest/v1";
/// DA-4: the disclosure message's own domain (`palw_da_disclosure_message_v4`).
pub const PALW_DA_DISCLOSURE_V4_DOMAIN: &[u8] = b"misaka-palw/da-disclosure-v4/message/v1";
/// DA-4: the discloser's ML-DSA-87 context — the second of `COMPLETE_V5`'s two additions.
pub const PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/da-disclosure-v4/mldsa87/v1";

// ---------------------------------------------------------------------------------------------
// Types (DA-2, DA-4; schema v22 rows 14–15, object tag 55)
// ---------------------------------------------------------------------------------------------

/// **One unit a session demands** (DA-2): an event `(row, tile)` of the claim's committed logits
/// trace, or a held unit (a prompt tile, a checkpoint's chunk, a step range or a step leaf).
///
/// Ordered (event units first, then held units by `PalwHeldMissingV1`'s order) because the claim's
/// `answered` set and every session's unit list are iterated in consensus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwDaUnitV1 {
    Event { row: u32, tile: u8 },
    Held(PalwHeldMissingV1),
}

impl PalwDaUnitV1 {
    /// The unit an event accusation's packed index names (`palw_da_event_index_parts_v1`).
    pub fn event_of_index(index: u32) -> Self {
        let (row, tile) = crate::palw_state_v2::palw_da_event_index_parts_v1(index);
        Self::Event { row, tile }
    }
}

/// **The claim's stage when a session opened** (DA-2): what its refuted cost is priced on (DA-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwDaStageV1 {
    /// `Provisional` (redrawn, a panel bound) or `PanelBound`.
    Live,
    /// `ReceiptLicensed`.
    Licensed,
    /// `Final`, its vesting row unmatured and unmoved.
    FinalRow,
}

/// **One open session**, keyed `(claim_id, accuser)` in `da_sessions` (DA-2, verbatim).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDaSessionV1 {
    pub opened_daa: u64,
    /// `opened_daa + W_disclose` (`window_challenge`, 1,200 on testnet-12).
    pub deadline_daa: u64,
    /// A seat of the claim's CURRENT panel when it opened; only these pause the claim (DA-5), and a
    /// redraw never clears it.
    pub accuser_is_seat: bool,
    /// DA-6's refuted cost, on the accuser's free half (A-6) while the session is open.
    pub exposure: u128,
    /// `[named, drawn…]` (DA-3), each distinct.
    pub units: Vec<PalwDaUnitV1>,
    pub stage: PalwDaStageV1,
}

/// **One claim's court record**, keyed by claim in `da_claims` (DA-2, verbatim): written with the
/// claim's first session, deleted when the claim record retires.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDaClaimV1 {
    pub open_seat_sessions: u8,
    pub open_other_sessions: u8,
    /// Non-seat sessions ever opened on the claim, capped at 16 (DA-8).
    pub opened_non_seat_total: u16,
    /// Sessions each seat ever opened on the claim, capped at 4 (DA-8).
    pub opened_by_seat: BTreeMap<PalwBondKeyV2, u8>,
    /// Set when the open SEAT count goes 0 → 1, cleared at 1 → 0 (DA-5).
    pub paused_since: Option<u64>,
    /// The last session close — DL-1's retirement re-arm.
    pub last_closed_daa: Option<u64>,
    /// Every unit answered on chain for this claim; an answer answers every session (DA-4).
    pub answered: BTreeSet<PalwDaUnitV1>,
    /// A `Flat` answer was accepted: it covers every in-run event unit (DA-4, IMPL-16).
    pub flat_answered: bool,
    /// Refuted exposure, `(accuser, amount)` in refutation order, held until the claim resolves:
    /// refunded at a conviction, burned when the record retires (DA-6).
    pub refuted_held: Vec<(PalwBondKeyV2, u128)>,
}

impl PalwDaClaimV1 {
    /// Sessions open on the claim, seat and non-seat.
    pub fn open_sessions(&self) -> u32 {
        u32::from(self.open_seat_sessions) + u32::from(self.open_other_sessions)
    }
}

/// **The answer a `MaterialDisclosedV2` carries** (DA-4): an event disclosure in the form the
/// class's scheme names, or a held unit's carriage. The carriage's own `claim` and `missing` must be
/// the object's claim and unit, and its own `signature` must be empty — the object's discloser
/// signs the whole answer (`palw_da_disclosure_message_v4`), so one answer has one encoding.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwDaAnswerV1 {
    Event(crate::palw_step_refute::PalwTraceEventDisclosureV1),
    Held(Box<PalwHeldDisclosureCarriageV1>),
}

impl PalwDaAnswerV1 {
    /// The binding every answer carries — what the identity rule (J-5) reads.
    pub fn binding(&self) -> &crate::palw_step_leg::PalwStepBindingV2 {
        match self {
            Self::Event(disclosure) => disclosure.binding(),
            Self::Held(carriage) => &carriage.binding,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Keys, digests and messages
// ---------------------------------------------------------------------------------------------

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **DA-7 / V-2b: the evidence half of a `DaDefault` key** — `H(PALW_DA_OFFENCE_KEY_DOMAIN_V1 ‖
/// claim_id)`, one per claim.
pub fn palw_da_offence_key_v1(claim_id: &Hash64) -> Hash64 {
    let mut state = keyed(PALW_DA_OFFENCE_KEY_DOMAIN_V1);
    state.update(claim_id.as_byte_slice());
    finish(state)
}

/// **The key a claim's `DaDefault` (kind 5) is recorded under** — `palw_offence_id_v1(DaDefault,
/// producer, palw_da_offence_key_v1(claim_id))` (N6). Public at admission, so R-3 refuses a reporter
/// commitment on it (V3S-03).
pub fn palw_da_offence_id_v1(producer: &crate::tx::TransactionOutpoint, claim_id: &Hash64) -> Hash64 {
    crate::palw_offence_v1::palw_offence_id_v1(
        crate::palw_offence_v1::PalwOffenceKindV1::DaDefault,
        producer,
        &palw_da_offence_key_v1(claim_id),
    )
}

/// **The digest a discloser signs**: the whole answer, keyed under its own domain, so the signature
/// covers every byte the fold reads.
pub fn palw_da_answer_digest_v1(answer: &PalwDaAnswerV1) -> Hash64 {
    let mut state = keyed(PALW_DA_ANSWER_DIGEST_DOMAIN_V1);
    let bytes = borsh::to_vec(answer).expect("an answer is borsh-serializable");
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(&bytes);
    finish(state)
}

/// **DA-4: what a discloser signs under [`PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT`]** — the network,
/// the claim, the unit, the answer's digest and the discloser. The discloser is inside the message
/// because any locked signer may answer (X7): a relayer must not be able to re-attribute one bond's
/// answer to another.
pub fn palw_da_disclosure_message_v4(
    network_domain: &Hash64,
    claim: &Hash64,
    unit: &PalwDaUnitV1,
    answer_digest: &Hash64,
    discloser: &PalwBondKeyV2,
) -> Hash64 {
    let mut state = keyed(PALW_DA_DISCLOSURE_V4_DOMAIN);
    state.update(network_domain.as_byte_slice());
    state.update(claim.as_byte_slice());
    state.update(&borsh::to_vec(unit).expect("a unit is borsh-serializable"));
    state.update(answer_digest.as_byte_slice());
    state.update(&borsh::to_vec(discloser).expect("a bond key is borsh-serializable"));
    finish(state)
}

// ---------------------------------------------------------------------------------------------
// DA-3: the draw — up to three more units, inside the committed run, seeded by the accepting block
// ---------------------------------------------------------------------------------------------

/// The tries a draw spends at most: a deterministic cap, so a space holding fewer free units than
/// the draw wants yields fewer (DA-3: "if the run has fewer than 3 further distinct units, fewer are
/// drawn") instead of looping.
pub const PALW_DA_DRAW_TRIES_V1: u64 = 64;

/// **The draw's seed** (DA-3): `H(PALW_DA_DRAW_DOMAIN_V1 ‖ ctx.block ‖ claim_id ‖ accuser)`, `ctx.block`
/// being the block that accepts the accusation. The accuser chooses none of it; a miner carrying its
/// own accusation can grind it only at one block's proof of work per try (§9.1 Q8).
pub fn palw_da_draw_seed_v1(block: &crate::BlockHash, claim_id: &Hash64, accuser: &PalwBondKeyV2) -> Hash64 {
    let mut state = keyed(PALW_DA_DRAW_DOMAIN_V1);
    state.update(b"seed");
    state.update(block.as_byte_slice());
    state.update(claim_id.as_byte_slice());
    state.update(&borsh::to_vec(accuser).expect("a bond key is borsh-serializable"));
    finish(state)
}

/// One uniform 64-bit word of the draw, the `counter`-th of stream `lane`.
fn draw_word(seed: &Hash64, lane: u8, counter: u64) -> u64 {
    let mut state = keyed(PALW_DA_DRAW_DOMAIN_V1);
    state.update(b"word");
    state.update(seed.as_byte_slice());
    state.update(&[lane]);
    state.update(&counter.to_le_bytes());
    u64::from_le_bytes(state.finalize().as_bytes()[..8].try_into().expect("8 bytes"))
}

/// `⌊word · n / 2^64⌋` — uniform in `[0, n)` to within `n / 2^64`; `0` for an empty range.
fn scaled(word: u64, n: u64) -> u64 {
    ((u128::from(word) * u128::from(n)) >> 64) as u64
}

/// **Where a draw may pick from** (DA-3). Every unit it names lies inside the committed run: the
/// fold builds it from chain facts (event rows) or from the accusation's authenticated binding (held
/// units), never from what the accuser names.
pub enum PalwDaDrawSpaceV1<'a> {
    /// No bound the fold can read — an event accusation of a held-context class's attempt: its held
    /// units are bounded by a binding the event object does not carry, and its answer discloses that
    /// binding for the accuser's next (held) session.
    Nothing,
    /// Event units: a row in `[0, rows)`, a tile in `[0, tiles)`, uniform over the pairs.
    Events { rows: u32, tiles: u32 },
    /// Prompt-id tiles `[0, tiles)`.
    PromptTiles { tiles: u64 },
    /// `StepRange { first, count: 1 }` over `[0, leaves)` (for a named `StepRange` or `StepLeaf`).
    StepRanges { leaves: u64 },
    /// `(checkpoint, chunk)`: a checkpoint uniform in `[0, checkpoints)`, then a chunk uniform in the
    /// chunks that checkpoint holds (`chunks_of`, from the binding).
    StateChunks { checkpoints: u32, chunks_of: &'a dyn Fn(u32) -> Option<u64> },
}

impl PalwDaDrawSpaceV1<'_> {
    /// Every unit, when the space holds at most `limit` (and is enumerable); else `None`.
    fn enumerate(&self, limit: u64) -> Option<Vec<PalwDaUnitV1>> {
        match self {
            Self::Nothing => Some(Vec::new()),
            Self::Events { rows, tiles } => {
                let total = u64::from(*rows) * u64::from(*tiles);
                (total <= limit).then(|| {
                    (0..*rows)
                        .flat_map(|row| (0..*tiles).map(move |tile| PalwDaUnitV1::Event { row, tile: tile.min(u32::from(u8::MAX)) as u8 }))
                        .collect()
                })
            }
            Self::PromptTiles { tiles } => (*tiles <= limit).then(|| {
                (0..*tiles).map(|tile| PalwDaUnitV1::Held(PalwHeldMissingV1::PromptIdsTile { tile: tile as u32 })).collect()
            }),
            Self::StepRanges { leaves } => (*leaves <= limit).then(|| {
                (0..*leaves).map(|first| PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first, count: 1 })).collect()
            }),
            Self::StateChunks { .. } => None,
        }
    }

    /// The `counter`-th candidate, or `None` when the candidate falls on nothing (a checkpoint with
    /// no chunks, an empty space).
    fn sample(&self, seed: &Hash64, counter: u64) -> Option<PalwDaUnitV1> {
        let word = draw_word(seed, 0, counter);
        match self {
            Self::Nothing => None,
            Self::Events { rows, tiles } => {
                let (rows, tiles) = (u64::from(*rows), u64::from(*tiles));
                let index = scaled(word, rows.checked_mul(tiles)?.max(1));
                (rows > 0 && tiles > 0).then(|| PalwDaUnitV1::Event { row: (index / tiles) as u32, tile: (index % tiles) as u8 })
            }
            Self::PromptTiles { tiles } => (*tiles > 0).then(|| {
                PalwDaUnitV1::Held(PalwHeldMissingV1::PromptIdsTile { tile: scaled(word, (*tiles).min(1 << 32)) as u32 })
            }),
            Self::StepRanges { leaves } => {
                (*leaves > 0).then(|| PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first: scaled(word, *leaves), count: 1 }))
            }
            Self::StateChunks { checkpoints, chunks_of } => {
                if *checkpoints == 0 {
                    return None;
                }
                let checkpoint = scaled(word, u64::from(*checkpoints)) as u32;
                let chunks = chunks_of(checkpoint).filter(|n| *n > 0)?;
                let chunk = scaled(draw_word(seed, 1, counter), chunks.min(1 << 32)) as u32;
                Some(PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint, chunk }))
            }
        }
    }
}

/// **DA-3: the drawn units of one session** — up to [`PALW_DA_DRAWN_UNITS_V1`] distinct units of
/// `space`, none equal to `named`, in draw order. Exact on a small space (every other unit, when
/// there are at most three), rejection-sampled with a deterministic cap otherwise.
pub fn palw_da_draw_units_v1(seed: &Hash64, named: &PalwDaUnitV1, space: &PalwDaDrawSpaceV1<'_>) -> Vec<PalwDaUnitV1> {
    let want = PALW_DA_DRAWN_UNITS_V1;
    if let Some(all) = space.enumerate(want as u64 + 1) {
        let others: Vec<PalwDaUnitV1> = all.into_iter().filter(|unit| unit != named).collect();
        if others.len() <= want {
            return others;
        }
    }
    let mut drawn: Vec<PalwDaUnitV1> = Vec::with_capacity(want);
    let mut counter = 0u64;
    while drawn.len() < want && counter < PALW_DA_DRAW_TRIES_V1 {
        if let Some(unit) = space.sample(seed, counter)
            && unit != *named
            && !drawn.contains(&unit)
        {
            drawn.push(unit);
        }
        counter += 1;
    }
    drawn
}

/// **The event rows a claim's committed run is known, from chain facts alone, to hold** (DA-3,
/// DA-4). An attempt's run past `palw_rcore_plus` is ONE decode row: `palw_attempt_job_v1` with the
/// prefill draw armed (a prerequisite of the fence through `palw_offence_attribution`) pins
/// `exact_decode_tokens = 1` (T64). A free-prompt run's decode count `D` is not in the claim record,
/// but under `palw_fp_da_pins` its `trace_chunk_count` is `⌈D / 256⌉` (ADR-0072 Decision 8), so every
/// row below `(count − 1) · 256 + 1 ≤ D` is in the run; without the pins only row 0 is known.
pub fn palw_da_in_run_rows_v1(claim: &crate::palw_state_v2::PalwClaimStateV2, fp_da_pins: bool) -> u32 {
    match claim.source {
        crate::palw_state_v2::PalwClaimSourceV2::Attempt => 1,
        crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. } if fp_da_pins => claim
            .trace_chunk_count
            .max(1)
            .saturating_sub(1)
            .saturating_mul(crate::palw_freeprompt_v3::PALW_FP_TRACE_CHUNK_EVENTS_V3)
            .saturating_add(1),
        crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. } => 1,
    }
}

/// **The tiles one event row is drawn over**: the row's tile count where the chain holds the class's
/// profile (a tiled scheme's `⌈vocab / PALW_LOGITS_TILE_LANES⌉`, at most 256; one for the flat
/// scheme), else one — tile 0, which every row of every scheme has, so the drawn unit stays inside
/// the run (the fold holds no class profile after registration, ADR-0119 Decision 2).
pub fn palw_da_row_tiles_v1(profile: Option<&crate::palw_step::PalwShapeProfileV3>) -> u32 {
    match profile {
        Some(profile) if profile.logits_scheme_id == crate::palw_step_refute::tiled_logits_scheme_id_v1() => {
            (profile.vocab_size as usize).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES).clamp(1, 256) as u32
        }
        _ => 1,
    }
}

/// **Is `unit` answered on chain for this claim?** (DA-4, IMPL-16.) Answered by name, or — an event
/// unit at tile 0 on a row the run is known to hold — by an accepted `Flat` answer, which opens every
/// row of the run (`palw_step_refute.rs`: a Flat disclosure is every row and every id; a row past
/// the decode count is refused there and answered by `OutOfRange`).
pub fn palw_da_unit_answered_v1(record: &PalwDaClaimV1, unit: &PalwDaUnitV1, in_run_rows: u32) -> bool {
    record.answered.contains(unit)
        || (record.flat_answered && matches!(unit, PalwDaUnitV1::Event { row, tile: 0 } if *row < in_run_rows))
}

/// **DA-3: is step leaf `leaf` of this binding a fused-attention site?** Such a leaf is never a DA
/// unit (`DaUnitNeedsDissection`): its terminal is ADR-0103 Decision 5's held dissection. The same
/// reading `palw_held_da_check_accusation_v1` refuses it by, spelled for the named refusal.
pub fn palw_da_step_leaf_is_fused_v1(binding: &crate::palw_step_leg::PalwStepBindingV2, leaf: u64) -> bool {
    crate::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf).is_some_and(|coord| {
        binding
            .shape_profile
            .resolve_node_slot(coord.node_slot)
            .is_some_and(|(node, _)| node.op_kind == crate::palw_step::PalwStepOpKindV1::AttnFused)
    })
}

// ---------------------------------------------------------------------------------------------
// DA-5: the pause credit — the ONE helper (V6's class-derived verification deadlines build on it)
// ---------------------------------------------------------------------------------------------

/// **DA-5: one anchor after a seat pause** — the anchor advanced by the part of the pause that the
/// phase it dates actually spent paused: `anchor + (now − max(paused_since, anchor))`. For a phase
/// that began before the pause this is the ADR's `anchor + (now − paused_since)`; for one that began
/// DURING it (a licence or a re-bind carried while a seat session was open) the clock never ran, so
/// the anchor comes back as `now` and the claim gets exactly the window it had left — the ADR's
/// intent ("the claim gets back exactly the challenge time it had left") rather than its letter,
/// which would credit the pre-licence part of the pause to the licence as well.
pub fn palw_da_pause_credit_v1(anchor: u64, paused_since: u64, now_daa: u64) -> u64 {
    anchor.saturating_add(now_daa.saturating_sub(paused_since.max(anchor)))
}

/// **DA-5: a claim's phase anchors after its open seat count went 1 → 0** — `rebound_daa` of a
/// `Provisional` claim (after a redraw), `bound_daa` of a `PanelBound` one, `licensed_daa` of a
/// `ReceiptLicensed` one, each through [`palw_da_pause_credit_v1`]. Nothing else moves:
/// `accepted_daa` anchors the retention obligation, and a terminal claim has no clock to credit.
pub fn palw_da_resume_claim_v1(
    claim: &crate::palw_state_v2::PalwClaimStateV2,
    paused_since: u64,
    now_daa: u64,
) -> crate::palw_state_v2::PalwClaimStateV2 {
    use crate::palw_state_v2::PalwClaimPhaseV2 as P;
    let credit = |anchor: u64| palw_da_pause_credit_v1(anchor, paused_since, now_daa);
    let mut resumed = claim.clone();
    match claim.phase {
        P::Provisional => resumed.rebound_daa = claim.rebound_daa.map(credit),
        P::PanelBound { bound_daa } => resumed.phase = P::PanelBound { bound_daa: credit(bound_daa) },
        P::ReceiptLicensed { licensed_daa } => resumed.phase = P::ReceiptLicensed { licensed_daa: credit(licensed_daa) },
        P::Final { .. } | P::Voided { .. } | P::DefaultDisputed { .. } => {}
    }
    resumed
}

// ---------------------------------------------------------------------------------------------
// DA-6: what a session costs
// ---------------------------------------------------------------------------------------------

/// **DA-6: a session's exposure** — `min(⌈r × S_P(stage)⌉, min_collateral_sompi)`, `r` = 1,000 bps,
/// `reward_base` being [`palw_da_stage_reward_base_v1`]'s.
pub fn palw_da_session_exposure_v1(reward_base: u128, min_collateral_sompi: u64) -> u128 {
    reward_base.saturating_mul(PALW_DA_REFUTED_COST_BPS_V1).div_ceil(10_000).min(u128::from(min_collateral_sompi))
}

/// **DA-6's `S_P(stage)`: the producer's nominal debit a default at `stage` triggers**, which is the
/// DA reward's base (R-1): `w + esc + rr` (`commitment_full`, the claim's whole reservation — after a
/// released escrow X7 takes `E` from uncommitted stake, so the total is the same) while `Live` or
/// `Licensed`; the producer's S3 action `min(25% · C_P, 3 G)` at `FinalRow` (the row burn is burned
/// vesting, never in a reward base).
pub fn palw_da_stage_reward_base_v1(stage: PalwDaStageV1, commitment_full: u128, producer_collateral: u64, g: u128) -> u128 {
    match stage {
        PalwDaStageV1::Live | PalwDaStageV1::Licensed => commitment_full,
        PalwDaStageV1::FinalRow => palw_da_producer_action_v1(producer_collateral, g),
    }
}

/// The S3/S4 action tier, `min(25% · C, 3 G)` (m = 3): what a post-`Final` default charges the
/// producer (S3) and every covering signer on top of its lock (S4).
pub fn palw_da_producer_action_v1(collateral: u64, g: u128) -> u128 {
    let quarter = u128::from(collateral) * u128::from(crate::palw_state_v2::PALW_RCORE_S3S4_ACTION_PERMILLE_V1) / 1000;
    quarter.min(g.saturating_mul(u128::from(crate::palw_state_v2::PALW_RCORE_ACTION_MULTIPLE_V1)))
}

// ---------------------------------------------------------------------------------------------
// DA-7 / C7: who a default charges
// ---------------------------------------------------------------------------------------------

/// **C7: does a `Valid` signer's recorded mask cover `unit`?** A full mask — a V1/V2 `Valid`, the full
/// seat — covers every unit. A partial mask covers no event unit (the ADR: "an event unit is covered
/// only by full masks unless T73 pins a leaf mapping for it"), and none of the held units either:
/// placing a step leaf in a segment needs the claim's committed step-leaf count, which no record the
/// fold keeps after the accusation carries (a partial seat is therefore never charged for a held
/// unit — an under-charge of a colluding partial seat, named, never an over-charge of an honest one).
pub fn palw_da_unit_covered_by_v1(_unit: &PalwDaUnitV1, attested: crate::palw_verification_v2::PalwSegmentMaskV2, segments: u16) -> bool {
    segments > 0 && attested.is_full(segments)
}

// ---------------------------------------------------------------------------------------------
// DA-4 / DA-8 (Phase 2, P2-7): an answer's form and size, and the ONE builder of tag 55
// ---------------------------------------------------------------------------------------------

/// **DA-4: is `answer` the form `unit` is answered in, on `claim`?** An event unit by an event
/// disclosure; a held unit by a version-1 carriage that names this claim and this unit and whose
/// own signature slot is empty (the object's discloser signs the whole answer, so one answer has
/// one encoding). The fold's own shape rule — `apply_da_answer_v1` refuses with
/// `DaAnswerMalformed { why }` and exactly this text — exported so node policy (P2-7) never builds a
/// form the fold refuses.
pub fn palw_da_answer_form_v1(claim: &Hash64, unit: &PalwDaUnitV1, answer: &PalwDaAnswerV1) -> Result<(), &'static str> {
    match (unit, answer) {
        (PalwDaUnitV1::Event { .. }, PalwDaAnswerV1::Event(_)) => Ok(()),
        (PalwDaUnitV1::Held(missing), PalwDaAnswerV1::Held(carriage)) => {
            if carriage.version != crate::palw_held_da_v1::PALW_HELD_DA_VERSION_V1 {
                Err("the carriage is not version 1")
            } else if carriage.claim != *claim || carriage.missing != *missing {
                Err("the carriage names another claim or unit")
            } else if !carriage.signature.is_empty() {
                Err("the carriage's own signature slot is empty: the discloser signs the whole answer")
            } else {
                Ok(())
            }
        }
        _ => Err("an event unit is answered by an event disclosure, a held unit by a held carriage"),
    }
}

/// **DA-8: an answer's size as the acceptance layer counts it** — `borsh(answer)` bytes, which the
/// gate compares against the ruleset's close ceiling (`PalwCourtParamsV2::max_close_bytes`, 80 KiB
/// on the frozen bundle) before it reads the signature. `u64::MAX` for an answer that does not
/// encode. One count for the gate and for the builder below, so a node never pays a carrier for an
/// answer the gate drops by size.
pub fn palw_da_answer_bytes_v1(answer: &PalwDaAnswerV1) -> u64 {
    borsh::to_vec(answer).map(|bytes| bytes.len() as u64).unwrap_or(u64::MAX)
}

/// **A held unit's answer as tag 55 carries it** (DA-4): the unit's carriage — this claim, this
/// unit, the binding the disclosure is checked under — with its own signature slot EMPTY, because
/// the object's discloser signs the whole answer ([`palw_da_disclosure_message_v4`]).
pub fn palw_da_held_answer_v1(
    claim: Hash64,
    missing: PalwHeldMissingV1,
    binding: crate::palw_step_leg::PalwStepBindingV2,
    disclosure: crate::palw_held_da_v1::PalwHeldDisclosureV1,
) -> PalwDaAnswerV1 {
    PalwDaAnswerV1::Held(Box::new(PalwHeldDisclosureCarriageV1 {
        version: crate::palw_held_da_v1::PALW_HELD_DA_VERSION_V1,
        claim,
        missing,
        binding,
        disclosure,
        signature: Vec::new(),
    }))
}

/// **A held unit's disclosure, built from a capture the answering node holds** (ADR-0103 Decision
/// 4, ADR-0111 Decisions 4 and 6; DA-4 for R-core's court): a leaf's evidence (which the fold then
/// adjudicates), a prompt tile, a state chunk or a run of step leaves — every unit the court can
/// name, with NO catch-all arm, so a unit added to [`PalwHeldMissingV1`] is a compile error here
/// rather than an honest producer or signer slashed for a silence it could not help.
///
/// The capture's lane does not matter: a free-prompt capture answers with its job's prompt, an
/// attempt capture with the canonical prompt its block's anchor implies (the fold draws held units
/// on attempt claims too, DA-3), and `roots` names the job either way. `binding` is asked only for
/// the three units whose answer does not carry its own (a leaf's evidence does): the caller reads it
/// off the capture in its family's form.
#[allow(clippy::too_many_arguments)]
pub fn palw_da_held_disclosure_from_capture_v1(
    backend: &dyn crate::palw_backend::PalwExecutionBackendV1,
    capture: &[u8],
    prompt_token_ids: &[u32],
    roots: crate::palw_backend::PalwClaimRootsV1,
    work_leaves: u64,
    missing: PalwHeldMissingV1,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    binding: impl FnOnce() -> Result<crate::palw_step_leg::PalwStepBindingV2, String>,
) -> Result<(crate::palw_step_leg::PalwStepBindingV2, crate::palw_held_da_v1::PalwHeldDisclosureV1), String> {
    use crate::palw_held_da_v1::PalwHeldDisclosureV1;
    Ok(match missing {
        PalwHeldMissingV1::StepLeaf { leaf } => {
            let evidence = crate::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
                backend,
                capture,
                prompt_token_ids,
                roots,
                work_leaves,
                leaf,
                form,
            )?;
            (evidence.refutation.binding.clone(), PalwHeldDisclosureV1::StepLeaf { evidence: Box::new(evidence) })
        }
        PalwHeldMissingV1::PromptIdsTile { tile } => {
            let binding = binding()?;
            let position = tile.saturating_mul(crate::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN);
            let opening = crate::palw_prompt_ids_v1::prompt_ids_opening_v1(prompt_token_ids, position)
                .map_err(|e| format!("the prompt tile does not open: {e}"))?;
            (binding, PalwHeldDisclosureV1::PromptIdsTile { opening })
        }
        PalwHeldMissingV1::StateChunk { checkpoint, chunk } => {
            let binding = binding()?;
            let (anchor, chunk) = backend.held_state_chunk_answer_v1(capture, prompt_token_ids, checkpoint, chunk)?;
            (binding, PalwHeldDisclosureV1::StateChunk { anchor, chunk })
        }
        PalwHeldMissingV1::StepRange { first, count } => {
            let binding = binding()?;
            let opening = backend.held_step_range_answer_v1(capture, prompt_token_ids, first, count)?;
            (binding, PalwHeldDisclosureV1::StepRange { opening })
        }
    })
}

/// **Why node policy builds no `MaterialDisclosedV2` from an answer** — each is a refusal the
/// acceptance layer or the fold would make of the object, found before a carrier is paid for.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwDaAnswerBuildErrorV1 {
    #[error("the fold refuses the answer's form: {0}")]
    Form(&'static str),
    #[error("the answer is {bytes} bytes, above this ruleset's {ceiling}-byte close ceiling (DA-8)")]
    AboveCloseCeiling { bytes: u64, ceiling: u64 },
    #[error("no signing key for the discloser")]
    Unsigned,
    #[error("the answer cannot ride a carrier: {0}")]
    CannotRide(&'static str),
}

/// **DA-4 (P2-7): the ONE builder of `MaterialDisclosedV2`** — what kaspad's responder queues for
/// every unit an open session demands of it, as the claim's producer or as a covering signer (X7).
///
/// It refuses what the chain would refuse, by the chain's own rules: the answer's form
/// ([`palw_da_answer_form_v1`], the fold's), its size against the ruleset's close ceiling
/// ([`palw_da_answer_bytes_v1`], the gate's), and the stateless ride rule
/// (`palw_lifecycle_object_may_ride_v2`: signed). It signs [`palw_da_disclosure_message_v4`] over
/// the answer's digest under [`PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT`] with `sign(message, context)`
/// — the discloser's key, which the gate verifies against the bond the object names. Whether that
/// bond may answer (the producer, or a live lock: X7) and whether a session still demands the unit
/// are state, read by the duty that asked (`palw_disclosure_duties_v1`) and by the fold again.
pub fn palw_da_answer_object_v1(
    network_domain: &Hash64,
    claim: Hash64,
    unit: PalwDaUnitV1,
    answer: PalwDaAnswerV1,
    discloser: PalwBondKeyV2,
    max_close_bytes: u64,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<crate::palw_state_v2::PalwConsensusObjectV2, PalwDaAnswerBuildErrorV1> {
    palw_da_answer_form_v1(&claim, &unit, &answer).map_err(PalwDaAnswerBuildErrorV1::Form)?;
    let bytes = palw_da_answer_bytes_v1(&answer);
    if bytes > max_close_bytes {
        return Err(PalwDaAnswerBuildErrorV1::AboveCloseCeiling { bytes, ceiling: max_close_bytes });
    }
    let message = palw_da_disclosure_message_v4(network_domain, &claim, &unit, &palw_da_answer_digest_v1(&answer), &discloser);
    let signature = sign(message.as_byte_slice(), PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT)
        .filter(|signature| !signature.is_empty())
        .ok_or(PalwDaAnswerBuildErrorV1::Unsigned)?;
    let object = crate::palw_state_v2::PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, answer, discloser, signature };
    crate::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(PalwDaAnswerBuildErrorV1::CannotRide)?;
    Ok(object)
}

/// **Does one `Flat` answer on this claim answer `unit` too?** (DA-4, IMPL-16.) The fold's own
/// predicate ([`palw_da_unit_answered_v1`]) asked of a record whose only fact is an accepted `Flat`:
/// an in-run event unit at tile 0. Node policy (P2-7) sends one `Flat` a claim and lets it answer
/// the rest, instead of paying a carrier for an answer the fold then refuses `DaUnitAlreadyAnswered`.
pub fn palw_da_flat_answers_unit_v1(unit: &PalwDaUnitV1, in_run_rows: u32) -> bool {
    palw_da_unit_answered_v1(&PalwDaClaimV1 { flat_answered: true, ..Default::default() }, unit, in_run_rows)
}

/// **DA-8 for retention (P2-7): can a session still demand a unit of `claim` at `now_daa` or
/// later?** A session opens only while `now + W_disclose ≤ trace_retention_daa` (the fold's
/// `da_admission_v1`), so none is open past `trace_retention_daa`; a voided claim's sessions are
/// released with it (`da_release_all_v1`) and no new one opens. So the material a unit is answered
/// from — the producer's capture, a covering signer's copy — is owed exactly while this holds, on
/// every phase that can carry a session, `Final` included (a `FinalRow` session is the S3 path).
/// Conservative at `Final`: a matured row stops new sessions earlier, never one already open.
///
/// Past `palw_rcore_plus` only: the caller reads the fence (below it ADR-0062's court decides by
/// the claim's phase, and a `Final` claim is never accused). Node policy's retention reads it — the
/// janitor for a producer's own free-prompt capture, the duty read for a signer's copy — so the two
/// cannot disagree with the court about how long a unit can be asked for.
pub fn palw_da_material_owed_v1(claim: &crate::palw_state_v2::PalwClaimStateV2, now_daa: u64) -> bool {
    !matches!(claim.phase, crate::palw_state_v2::PalwClaimPhaseV2::Voided { .. }) && now_daa <= claim.trace_retention_daa
}

/// **C-9 (P2-6): the unit an automatic accusation names** — the event its seat's `Unavailable`
/// receipt names (`chunk_index: 0`): the run's first row, tile 0. A seat that was served nothing
/// holds no binding, so it cannot name a held unit or a divergent leaf (those are P2-8d's, named from
/// a disclosed binding); the fold draws the rest inside the committed run (DA-3), which is what makes
/// one named unit enough. Row 0 is inside the fold's bound for every claim that pins a chunk
/// (`palw_da_max_accusable_rows_v1`: `trace_chunk_count` is 1 on the attempt lane and `⌈rows/256⌉`
/// on the free-prompt lane), and inside the run for every claim with a decode row, so an honest
/// producer answers it with the run's own `Flat`, never `OutOfRange`. The node's read
/// (`palw_producer_v2::palw_da_accusation_check_v1`) asks the fold's bound anyway.
pub const PALW_DA_AUTO_NAMED_UNIT_V1: PalwDaUnitV1 = PalwDaUnitV1::Event { row: 0, tile: 0 };

/// **Why node policy builds no `DefaultAccused`** — each a refusal the acceptance layer would make
/// of the object, found before a carrier is paid for.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwDaAccusationBuildErrorV1 {
    /// `DefaultAccused` names an event; a held unit rides `DefaultAccusedHeld` with a binding.
    #[error("an event accusation names an event unit, not {0:?}")]
    NotAnEvent(PalwDaUnitV1),
    /// The row does not survive `palw_da_event_index_v1`'s packing (`row << 8 | tile`), so the
    /// object would name another event than the one asked for.
    #[error("event {0:?} does not pack into an accusation's index")]
    Unpackable(PalwDaUnitV1),
    #[error("no signing key for the accuser")]
    Unsigned,
    #[error("the accusation cannot ride a carrier: {0}")]
    CannotRide(&'static str),
}

/// **DA-1 / DA-3 (P2-6): the ONE builder of an event `DefaultAccused`** — what kaspad's seat files
/// beside its `Unavailable` receipt, and when a licence lands on a claim that never served it.
///
/// It packs the unit as the fold unpacks it (`PalwDaUnitV1::event_of_index`, refusing a row the
/// packing would move), signs `palw_da_accusation_message_v2` over the network, the claim, the index
/// and the accuser under `PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT` with `sign(message, context)` — the
/// accuser's key, which the acceptance layer verifies against the bond the object names — and holds
/// the object to the stateless ride rule (`palw_lifecycle_object_may_ride_v2`: signed). Whether the
/// fold opens a session for it (C-8: the claim accusable, the accuser's standing, DA-8's caps and
/// A-6's room) is state, read by `palw_producer_v2::palw_da_accusation_check_v1` before this is
/// called and by the fold again.
pub fn palw_da_accusation_object_v1(
    network_domain: &Hash64,
    claim: Hash64,
    unit: PalwDaUnitV1,
    accuser: PalwBondKeyV2,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<crate::palw_state_v2::PalwConsensusObjectV2, PalwDaAccusationBuildErrorV1> {
    let PalwDaUnitV1::Event { row, tile } = unit else { return Err(PalwDaAccusationBuildErrorV1::NotAnEvent(unit)) };
    let index = crate::palw_state_v2::palw_da_event_index_v1(row, tile);
    if PalwDaUnitV1::event_of_index(index) != unit {
        return Err(PalwDaAccusationBuildErrorV1::Unpackable(unit));
    }
    let message = crate::palw_state_v2::palw_da_accusation_message_v2(*network_domain, &claim, index, &accuser);
    let signature = sign(message.as_byte_slice(), crate::palw_state_v2::PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT)
        .filter(|signature| !signature.is_empty())
        .ok_or(PalwDaAccusationBuildErrorV1::Unsigned)?;
    let object = crate::palw_state_v2::PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: index, accuser, signature };
    crate::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(PalwDaAccusationBuildErrorV1::CannotRide)?;
    Ok(object)
}

/// Every domain and context this module keys — listed in `PALW_STATE_V2_ALL_DOMAINS` (the family
/// the cross-family uniqueness sweep and the committed context set are derived from).
pub const PALW_DA_RCORE_ALL_DOMAINS: &[&[u8]] = &[
    PALW_DA_DRAW_DOMAIN_V1,
    PALW_DA_OFFENCE_KEY_DOMAIN_V1,
    PALW_DA_ANSWER_DIGEST_DOMAIN_V1,
    PALW_DA_DISCLOSURE_V4_DOMAIN,
    PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(u64::from(n)), index: 0 })
    }

    /// **The DA-2 records encode field by field in the ADR's order** (the v22 layout): a session's
    /// bytes are its six fields concatenated, a claim record's its nine, and each enum sits at its
    /// frozen index — events before held units, the stages `Live, Licensed, FinalRow`.
    #[test]
    fn the_da_records_encode_in_the_adrs_field_order() {
        assert_eq!(borsh::to_vec(&PalwDaStageV1::Live).unwrap(), vec![0]);
        assert_eq!(borsh::to_vec(&PalwDaStageV1::Licensed).unwrap(), vec![1]);
        assert_eq!(borsh::to_vec(&PalwDaStageV1::FinalRow).unwrap(), vec![2]);
        assert_eq!(borsh::to_vec(&PalwDaUnitV1::Event { row: 0x0102_0304, tile: 5 }).unwrap(), vec![0, 4, 3, 2, 1, 5]);
        let held = PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 7 });
        let mut expected = vec![1];
        expected.extend(borsh::to_vec(&PalwHeldMissingV1::StepLeaf { leaf: 7 }).unwrap());
        assert_eq!(borsh::to_vec(&held).unwrap(), expected);
        assert!(PalwDaUnitV1::Event { row: u32::MAX, tile: u8::MAX } < held, "every event unit sorts before every held unit");

        let session = PalwDaSessionV1 {
            opened_daa: 11,
            deadline_daa: 1_211,
            accuser_is_seat: true,
            exposure: 0x0A0B,
            units: vec![PalwDaUnitV1::Event { row: 0, tile: 0 }, held],
            stage: PalwDaStageV1::Licensed,
        };
        let flat = (
            session.opened_daa,
            session.deadline_daa,
            session.accuser_is_seat,
            session.exposure,
            session.units.clone(),
            session.stage,
        );
        assert_eq!(borsh::to_vec(&session).unwrap(), borsh::to_vec(&flat).unwrap(), "the session is its six fields in order");
        assert_eq!(borsh::from_slice::<PalwDaSessionV1>(&borsh::to_vec(&session).unwrap()).unwrap(), session);

        let record = PalwDaClaimV1 {
            open_seat_sessions: 1,
            open_other_sessions: 2,
            opened_non_seat_total: 3,
            opened_by_seat: BTreeMap::from([(bond(1), 4)]),
            paused_since: Some(5),
            last_closed_daa: Some(6),
            answered: BTreeSet::from([held]),
            flat_answered: true,
            refuted_held: vec![(bond(2), 7)],
        };
        let flat = (
            record.open_seat_sessions,
            record.open_other_sessions,
            record.opened_non_seat_total,
            record.opened_by_seat.clone(),
            record.paused_since,
            record.last_closed_daa,
            record.answered.clone(),
            record.flat_answered,
            record.refuted_held.clone(),
        );
        assert_eq!(borsh::to_vec(&record).unwrap(), borsh::to_vec(&flat).unwrap(), "the claim record is its nine fields in order");
        assert_eq!(borsh::from_slice::<PalwDaClaimV1>(&borsh::to_vec(&record).unwrap()).unwrap(), record);
        assert_eq!(record.open_sessions(), 3);
    }

    /// **DA-3: the draw is seeded by the accepting block, stays inside its space, never repeats a
    /// unit or the named one, and draws every other unit of a small space.**
    #[test]
    fn the_draw_is_seeded_by_the_block_bounded_distinct_and_exact_on_a_small_space() {
        let claim = Hash64::from_bytes([7; 64]);
        let seed = |block: u8| palw_da_draw_seed_v1(&Hash64::from_bytes([block; 64]), &claim, &bond(1));
        assert_ne!(seed(1), seed(2), "the accepting block moves the seed");
        assert_ne!(palw_da_draw_seed_v1(&Hash64::from_bytes([1; 64]), &claim, &bond(2)), seed(1), "and so does the accuser");
        // The attempt run: one row, one tile.
        let one = PalwDaDrawSpaceV1::Events { rows: 1, tiles: 1 };
        assert_eq!(palw_da_draw_units_v1(&seed(1), &PalwDaUnitV1::Event { row: 5, tile: 0 }, &one), vec![PalwDaUnitV1::Event {
            row: 0,
            tile: 0
        }]);
        assert!(palw_da_draw_units_v1(&seed(1), &PalwDaUnitV1::Event { row: 0, tile: 0 }, &one).is_empty());
        assert!(palw_da_draw_units_v1(&seed(1), &PalwDaUnitV1::Event { row: 0, tile: 0 }, &PalwDaDrawSpaceV1::Nothing).is_empty());
        // A wide run: three distinct in-run units, none the named one, the same on every node.
        let wide = PalwDaDrawSpaceV1::Events { rows: 300, tiles: 4 };
        let named = PalwDaUnitV1::Event { row: 9, tile: 1 };
        for block in 0..32u8 {
            let drawn = palw_da_draw_units_v1(&seed(block), &named, &wide);
            assert_eq!(drawn.len(), PALW_DA_DRAWN_UNITS_V1);
            assert_eq!(drawn, palw_da_draw_units_v1(&seed(block), &named, &wide), "deterministic");
            let distinct: BTreeSet<_> = drawn.iter().collect();
            assert_eq!(distinct.len(), drawn.len());
            for unit in &drawn {
                assert_ne!(*unit, named);
                assert!(matches!(unit, PalwDaUnitV1::Event { row, tile } if *row < 300 && *tile < 4), "{unit:?}");
            }
        }
        // Held spaces: width-1 ranges over two leaves (both drawn), prompt tiles, state chunks.
        let leaf = PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 0 });
        assert_eq!(palw_da_draw_units_v1(&seed(1), &leaf, &PalwDaDrawSpaceV1::StepRanges { leaves: 2 }), vec![
            PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first: 0, count: 1 }),
            PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first: 1, count: 1 }),
        ]);
        let tiles = palw_da_draw_units_v1(
            &seed(3),
            &PalwDaUnitV1::Held(PalwHeldMissingV1::PromptIdsTile { tile: 0 }),
            &PalwDaDrawSpaceV1::PromptTiles { tiles: 100 },
        );
        assert_eq!(tiles.len(), 3);
        assert!(tiles.iter().all(|u| matches!(u, PalwDaUnitV1::Held(PalwHeldMissingV1::PromptIdsTile { tile }) if *tile < 100 && *tile != 0)));
        let chunks_of = |checkpoint: u32| -> Option<u64> { (checkpoint > 0).then_some(u64::from(checkpoint) * 2) };
        let chunks = palw_da_draw_units_v1(
            &seed(4),
            &PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint: 1, chunk: 0 }),
            &PalwDaDrawSpaceV1::StateChunks { checkpoints: 6, chunks_of: &chunks_of },
        );
        assert_eq!(chunks.len(), 3);
        for unit in &chunks {
            let PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint, chunk }) = unit else { panic!("{unit:?}") };
            assert!(*checkpoint > 0 && *checkpoint < 6 && u64::from(*chunk) < u64::from(*checkpoint) * 2, "{unit:?}");
        }
    }

    /// **DA-5: the pause credit returns exactly the time the phase spent paused** — the whole pause
    /// to a phase dated before it, the part since its own start to one that began during it (the
    /// clock never ran for it), nothing to a zero-length pause.
    #[test]
    fn the_pause_credit_returns_exactly_the_time_the_phase_spent_paused() {
        assert_eq!(palw_da_pause_credit_v1(100, 150, 300), 250, "a phase before the pause: + (now − since)");
        assert_eq!(palw_da_pause_credit_v1(200, 150, 300), 300, "a phase inside the pause restarts at now");
        assert_eq!(palw_da_pause_credit_v1(100, 150, 150), 100, "no pause, no credit");
    }

    /// **DA-6: `min(⌈r · S_P(stage)⌉, min_collateral)`**, the floor's ADR figures: `Live`/`Licensed`
    /// on the commitment (3,201.0 MSK → 320.10 MSK), `FinalRow` on the producer's S3 action
    /// `min(25% · C, 3 G)` (13k producer: 3,250 → 325.00), the cap at `min_collateral`.
    #[test]
    fn a_session_costs_r_times_its_stage_base_and_never_more_than_the_floor() {
        const MSK: u128 = 100_000_000;
        let floor = 13_000 * 100_000_000u64;
        let live = palw_da_stage_reward_base_v1(PalwDaStageV1::Live, 320_100_000_000, floor, u128::MAX / 4);
        assert_eq!(palw_da_session_exposure_v1(live, floor), 32_010_000_000, "320.10 MSK");
        let final_row = palw_da_stage_reward_base_v1(PalwDaStageV1::FinalRow, 0, floor, 10_000 * MSK);
        assert_eq!(final_row, 3_250 * MSK, "min(25% x 13,000, 3G)");
        assert_eq!(palw_da_session_exposure_v1(final_row, floor), 325 * MSK);
        assert_eq!(palw_da_producer_action_v1(939_000 * 100_000_000, 100 * MSK), 300 * MSK, "3G binds");
        assert_eq!(palw_da_session_exposure_v1(1_000_000 * MSK, floor), u128::from(floor), "capped at min_collateral");
        assert_eq!(palw_da_session_exposure_v1(1, floor), 1, "rounded up: never free");
    }

    /// **C7: only a full mask covers a unit** — a V1/V2 `Valid` and the full seat cover every unit; a
    /// partial mask covers no event unit and no held unit (the fold holds no step-leaf count to place
    /// one in a segment).
    #[test]
    fn only_a_full_mask_covers_a_da_unit() {
        use crate::palw_verification_v2::PalwSegmentMaskV2;
        let event = PalwDaUnitV1::Event { row: 0, tile: 0 };
        let leaf = PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 3 });
        for unit in [event, leaf] {
            assert!(palw_da_unit_covered_by_v1(&unit, PalwSegmentMaskV2::full(4), 4));
            assert!(!palw_da_unit_covered_by_v1(&unit, PalwSegmentMaskV2(0b0001), 4));
            assert!(!palw_da_unit_covered_by_v1(&unit, PalwSegmentMaskV2::NONE, 0), "an unwritten mask covers nothing");
        }
    }

    /// **P2-7: one `Flat` answers exactly what the fold says it answers** — an in-run event unit at
    /// tile 0 (IMPL-16), never a row past the run, another tile or a held unit — so the responder
    /// skips only what the fold would refuse `DaUnitAlreadyAnswered`.
    #[test]
    fn one_flat_answers_exactly_the_in_run_event_units_at_tile_0() {
        assert!(palw_da_flat_answers_unit_v1(&PalwDaUnitV1::Event { row: 0, tile: 0 }, 1));
        assert!(!palw_da_flat_answers_unit_v1(&PalwDaUnitV1::Event { row: 1, tile: 0 }, 1), "past the one-row run");
        assert!(palw_da_flat_answers_unit_v1(&PalwDaUnitV1::Event { row: 256, tile: 0 }, 257));
        assert!(!palw_da_flat_answers_unit_v1(&PalwDaUnitV1::Event { row: 0, tile: 1 }, 1), "another tile");
        assert!(!palw_da_flat_answers_unit_v1(&PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 0 }), 1), "a held unit");
    }

    /// **The keys and the message separate what they bind.** The offence key is per claim; the
    /// message moves with every field — the network, the claim, the unit, the answer and the
    /// discloser — so a relayer cannot move an answer to another unit or another signer.
    #[test]
    fn the_da_keys_and_the_disclosure_message_bind_every_field() {
        let c1 = Hash64::from_bytes([1; 64]);
        let c2 = Hash64::from_bytes([2; 64]);
        assert_ne!(palw_da_offence_key_v1(&c1), palw_da_offence_key_v1(&c2));
        let producer = bond(9);
        assert_eq!(
            palw_da_offence_id_v1(&producer.0, &c1),
            crate::palw_offence_v1::palw_offence_id_v1(
                crate::palw_offence_v1::PalwOffenceKindV1::DaDefault,
                &producer.0,
                &palw_da_offence_key_v1(&c1)
            )
        );
        let net = Hash64::from_bytes([3; 64]);
        let digest = Hash64::from_bytes([4; 64]);
        let unit = PalwDaUnitV1::Event { row: 0, tile: 0 };
        let base = palw_da_disclosure_message_v4(&net, &c1, &unit, &digest, &bond(1));
        let moved = [
            palw_da_disclosure_message_v4(&Hash64::from_bytes([5; 64]), &c1, &unit, &digest, &bond(1)),
            palw_da_disclosure_message_v4(&net, &c2, &unit, &digest, &bond(1)),
            palw_da_disclosure_message_v4(&net, &c1, &PalwDaUnitV1::Event { row: 0, tile: 1 }, &digest, &bond(1)),
            palw_da_disclosure_message_v4(&net, &c1, &unit, &Hash64::from_bytes([6; 64]), &bond(1)),
            palw_da_disclosure_message_v4(&net, &c1, &unit, &digest, &bond(2)),
        ];
        let distinct: BTreeSet<Hash64> = moved.iter().copied().chain(std::iter::once(base)).collect();
        assert_eq!(distinct.len(), moved.len() + 1, "every field moves the message");
        let names: BTreeSet<&[u8]> = PALW_DA_RCORE_ALL_DOMAINS.iter().copied().collect();
        assert_eq!(names.len(), PALW_DA_RCORE_ALL_DOMAINS.len(), "five distinct domains");
    }

    /// **P2-6: the ONE `DefaultAccused` builder signs what the acceptance layer verifies and names
    /// the unit the fold reads back.** The automatic unit packs to index 0 and unpacks to itself; the
    /// signature is asked over `palw_da_accusation_message_v2` of exactly the network, claim, index
    /// and accuser, under the accusation context; a held unit, a row the packing would move, and a
    /// missing or empty signature build nothing.
    #[test]
    fn p2_6_the_accusation_builder_signs_the_packed_unit_and_refuses_what_would_not_ride() {
        use crate::palw_state_v2::{PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT, PalwConsensusObjectV2, palw_da_accusation_message_v2};
        let net = Hash64::from_bytes([3; 64]);
        let claim = Hash64::from_bytes([1; 64]);
        let mut asked: Option<(Vec<u8>, Vec<u8>)> = None;
        let object = palw_da_accusation_object_v1(&net, claim, PALW_DA_AUTO_NAMED_UNIT_V1, bond(4), |message, context| {
            asked = Some((message.to_vec(), context.to_vec()));
            Some(vec![0xA5; 8])
        })
        .expect("built");
        let PalwConsensusObjectV2::DefaultAccused { claim: named, missing_event_index, accuser, signature } = &object else {
            panic!("a DefaultAccused: {object:?}")
        };
        assert_eq!((*named, *missing_event_index, *accuser, signature.clone()), (claim, 0, bond(4), vec![0xA5; 8]));
        assert_eq!(PalwDaUnitV1::event_of_index(*missing_event_index), PALW_DA_AUTO_NAMED_UNIT_V1, "the fold reads the same unit");
        let message = palw_da_accusation_message_v2(net, &claim, 0, &bond(4));
        assert_eq!(asked, Some((message.as_byte_slice().to_vec(), PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT.to_vec())));
        let tiled = PalwDaUnitV1::Event { row: 7, tile: 2 };
        let PalwConsensusObjectV2::DefaultAccused { missing_event_index, .. } =
            palw_da_accusation_object_v1(&net, claim, tiled, bond(4), |_, _| Some(vec![1])).expect("built")
        else {
            unreachable!()
        };
        assert_eq!(PalwDaUnitV1::event_of_index(missing_event_index), tiled);
        let held = PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 1 });
        assert_eq!(
            palw_da_accusation_object_v1(&net, claim, held, bond(4), |_, _| Some(vec![1])),
            Err(PalwDaAccusationBuildErrorV1::NotAnEvent(held))
        );
        let wide = PalwDaUnitV1::Event { row: 1 << 24, tile: 0 };
        assert_eq!(
            palw_da_accusation_object_v1(&net, claim, wide, bond(4), |_, _| Some(vec![1])),
            Err(PalwDaAccusationBuildErrorV1::Unpackable(wide))
        );
        for unsigned in [None, Some(Vec::new())] {
            assert_eq!(
                palw_da_accusation_object_v1(&net, claim, PALW_DA_AUTO_NAMED_UNIT_V1, bond(4), |_, _| unsigned.clone()),
                Err(PalwDaAccusationBuildErrorV1::Unsigned)
            );
        }
    }
}
