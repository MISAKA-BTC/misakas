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
/// **Have seats landed automatic DA answering?** — `false` in this build. DA-7's S4 on covering
/// signers and N9's `ProducerWithholding` against them are fair only because a covering signer can
/// answer the demanded unit itself with the material it retained (DA-4, X7: any bond with a live
/// lock on the claim may disclose). While kaspad's seats do not yet do that, both stay DORMANT
/// ([`crate::palw_state_v2::palw_da_signer_liability_armed_v1`] reads this const); the producer's
/// DA-7 charge, the `DaDefault` record, the reward and every other DA rule stay live.
///
/// **Owned by the peer's Phase 2, P2-7**, which flips it to `true` together with its tests when
/// kaspad seats auto-answer DA units with retained material as covering signers. A consensus
/// constant, not a switch: the fold reads it through `PalwTransitionExtrasV1::seat_da_answer_landed`,
/// which the processor sets to exactly this value, and which only tests set otherwise.
pub const PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1: bool = false;
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
}
