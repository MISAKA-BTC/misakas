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
