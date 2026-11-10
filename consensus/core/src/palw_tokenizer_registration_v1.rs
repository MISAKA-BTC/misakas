//! RFC-0051 preclaim tokenizer registration wire and deterministic pricing.
//! The caller supplies adopted policy and verifies the accepted state, bond,
//! reservations and UTXO fee before a fold may create a pending registration.

use crate::{
    Hash64,
    mldsa87_primitives::MLDSA87_SIGNATURE_LEN,
    palw_court_service_v1::ServiceCost,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1,
    palw_state_v2::PalwBondKeyV2,
    palw_step_leg::PalwStepOpeningV1,
    palw_tokenizer_rendering_v2::{PalwTfp1TokenizerIntakePinsV2, PalwTokenizerRenderingStatementV2, ROOTED_TOKENIZER_LIMITS_V1},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};

pub const TOKENIZER_REGISTRATION_VERSION_V1: u16 = 1;
pub const TOKENIZER_REGISTRATION_PART_BYTES_V1: usize =
    crate::palw_tokenizer_artifact_inventory_v1::PALW_ARTIFACT_INVENTORY_PART_BYTES_V1;
pub const TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1: &[u8] = b"misaka-palw/tokenizer-registration/open/v1/mldsa87";
pub const TOKENIZER_REGISTRATION_PART_CONTEXT_V1: &[u8] = b"misaka-palw/tokenizer-registration/part/v1/mldsa87";
pub const TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1: &[u8] = b"misaka-palw/tokenizer-registration/finish/v1/mldsa87";
pub const TOKENIZER_REGISTRATION_FINISH_ATTEMPTS_V1: u64 = 2;

// Isolated UTXO projection, optional Positive fee-plan rehearsal and UTXO
// replay, observed rehearsal, and authoritative fold. Only the two UTXO
// passes can abandon a Round body and undo its private tokenizer journal.
const TOKENIZER_REGISTRATION_NATIVE_PASSES_V1: u64 = 5;
const TOKENIZER_REGISTRATION_ROLLBACK_PASSES_V1: u64 = 2;
// Canonical parent, session, slot and EOG are fixed across those passes.
const TOKENIZER_REGISTRATION_FINISH_PARSER_PASSES_V1: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationOpenBodyV1 {
    pub version: u16,
    pub network: Hash64,
    pub policy: Hash64,
    pub pins: PalwTfp1TokenizerIntakePinsV2,
    pub artifact_root: Hash64,
    pub statement: PalwTokenizerRenderingStatementV2,
    pub eog_digest: Hash64,
    pub opener: PalwBondKeyV2,
    pub nonce: Hash64,
    pub accept_until_daa: u64,
    pub finish_until_daa: u64,
    pub material_until_daa: u64,
    pub max_registration_charge: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TokenizerRegistrationOpenV1 {
    pub body: TokenizerRegistrationOpenBodyV1,
    pub signature: [u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationPartBodyV1 {
    pub version: u16,
    pub network: Hash64,
    pub policy: Hash64,
    pub open_id: Hash64,
    pub opener: PalwBondKeyV2,
    pub index: u32,
    pub offset: u64,
    pub payload_digest: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TokenizerRegistrationPartV1 {
    pub body: TokenizerRegistrationPartBodyV1,
    pub signature: [u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationFinishBodyV1 {
    pub version: u16,
    pub network: Hash64,
    pub policy: Hash64,
    pub open_id: Hash64,
    pub opener: PalwBondKeyV2,
    pub eog_digest: Hash64,
    pub finish_until_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TokenizerRegistrationFinishV1 {
    pub body: TokenizerRegistrationFinishBodyV1,
    pub signature: [u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    pub eog: PalwTokenTableEogOpeningV1,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenizerRegistrationWireV1 {
    Open(TokenizerRegistrationOpenV1),
    Part(TokenizerRegistrationPartV1),
    Finish(TokenizerRegistrationFinishV1),
}

fn check_scope(version: u16, network: Hash64, policy: Hash64, opener: PalwBondKeyV2) -> Result<(), &'static str> {
    if version != TOKENIZER_REGISTRATION_VERSION_V1 || [network, policy, opener.0.transaction_id].contains(&Hash64::default()) {
        return Err("invalid tokenizer registration version, network, policy or opener");
    }
    Ok(())
}

fn verify(message: Hash64, key: &[u8], signature: &[u8], context: &[u8]) -> Result<(), String> {
    if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(key, message.as_byte_slice(), signature, context) {
        return Err("invalid tokenizer registration opener signature".into());
    }
    Ok(())
}

impl TokenizerRegistrationOpenV1 {
    pub fn signing_message_v1(&self) -> Result<Hash64, String> {
        self.body.id_v1()
    }

    pub fn verify_signature_v1(&self, key: &[u8]) -> Result<(), String> {
        verify(self.signing_message_v1()?, key, &self.signature, TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1)
    }
}

impl TokenizerRegistrationOpenBodyV1 {
    pub fn check_v1(&self) -> Result<(), &'static str> {
        check_scope(self.version, self.network, self.policy, self.opener)?;
        let p = self.pins;
        let s = self.statement;
        if [
            p.class_id,
            p.container_root,
            p.semantics_id,
            p.inventory_id,
            self.artifact_root,
            s.tokenizer_id,
            s.table_root,
            self.eog_digest,
            self.nonce,
        ]
        .contains(&Hash64::default())
            || !(1..=ROOTED_TOKENIZER_LIMITS_V1.raw_bytes).contains(&s.exact_raw_bytes)
            || !(1..=ROOTED_TOKENIZER_LIMITS_V1.vocab).contains(&s.vocab)
            || self.accept_until_daa == 0
            || self.accept_until_daa > self.finish_until_daa
            || self.finish_until_daa >= self.material_until_daa
            || self.max_registration_charge == 0
        {
            return Err("invalid tokenizer registration pins, bounds, fee cap or clocks");
        }
        Ok(())
    }

    pub fn id_v1(&self) -> Result<Hash64, String> {
        self.check_v1()?;
        stream_digest(b"misaka-palw/tokenizer-registration/open/v1", self)
    }

    pub fn charge_id_v1(&self) -> Result<Hash64, String> {
        stream_digest(b"misaka-palw/tokenizer-registration/charge/v1", &(self.id_v1()?, self.policy))
    }
}

impl TokenizerRegistrationPartV1 {
    pub fn payload_digest_v1(bytes: &[u8]) -> Result<Hash64, String> {
        stream_digest(b"misaka-palw/tokenizer-registration/part-bytes/v1", &bytes)
    }

    pub fn signing_message_v1(&self) -> Result<Hash64, String> {
        let b = self.body;
        check_scope(b.version, b.network, b.policy, b.opener)?;
        if b.open_id == Hash64::default()
            || self.bytes.is_empty()
            || self.bytes.len() > TOKENIZER_REGISTRATION_PART_BYTES_V1
            || b.payload_digest != Self::payload_digest_v1(&self.bytes)?
        {
            return Err("tokenizer registration part has invalid open ID, length or payload digest".into());
        }
        stream_digest(b"misaka-palw/tokenizer-registration/part/v1", &b)
    }

    pub fn check_open_v1(&self, open: &TokenizerRegistrationOpenBodyV1) -> Result<(), String> {
        let b = self.body;
        let (offset, bytes) = TokenizerRegistrationGeometryV1::new(open.statement.exact_raw_bytes)?.part_v1(b.index)?;
        if (b.network, b.policy, b.open_id, b.opener, b.offset, self.bytes.len())
            != (open.network, open.policy, open.id_v1()?, open.opener, offset, bytes)
        {
            return Err("tokenizer registration part substitutes its open or indexed extent".into());
        }
        self.signing_message_v1().map(|_| ())
    }

    pub fn verify_signature_v1(&self, key: &[u8]) -> Result<(), String> {
        verify(self.signing_message_v1()?, key, &self.signature, TOKENIZER_REGISTRATION_PART_CONTEXT_V1)
    }
}

impl TokenizerRegistrationFinishV1 {
    pub fn eog_digest_v1(eog: &PalwTokenTableEogOpeningV1) -> Result<Hash64, String> {
        stream_digest(b"misaka-palw/tokenizer-registration/eog/v1", eog)
    }

    pub fn signing_message_v1(&self) -> Result<Hash64, String> {
        let b = self.body;
        check_scope(b.version, b.network, b.policy, b.opener)?;
        if b.open_id == Hash64::default()
            || b.finish_until_daa == 0
            || self.eog.ids.is_empty()
            || self.eog.ids.len() > ROOTED_TOKENIZER_LIMITS_V1.eog_ids as usize
            || self.eog.opening.siblings.len() > EOG_SIBLINGS_MAX
            || b.eog_digest != Self::eog_digest_v1(&self.eog)?
        {
            return Err("tokenizer registration finish has invalid open ID, clock or EOG opening".into());
        }
        stream_digest(b"misaka-palw/tokenizer-registration/finish/v1", &b)
    }

    pub fn check_open_v1(&self, open: &TokenizerRegistrationOpenBodyV1, policy: &TokenizerRegistrationPolicyV1) -> Result<(), String> {
        let b = self.body;
        if (b.network, b.policy, b.open_id, b.opener, b.finish_until_daa, b.eog_digest)
            != (open.network, open.policy, open.id_v1()?, open.opener, open.finish_until_daa, open.eog_digest)
            || b.policy != policy.id_v1()?
            || self.eog.ids.len() > policy.max_eog_ids as usize
            || self.eog.ids.iter().any(|&id| id >= open.statement.vocab)
            || !self.eog.verifies(&open.statement.table_root, open.statement.vocab)
        {
            return Err("tokenizer registration finish substitutes its open or registered EOG table".into());
        }
        self.signing_message_v1().map(|_| ())
    }

    pub fn verify_signature_v1(&self, key: &[u8]) -> Result<(), String> {
        verify(self.signing_message_v1()?, key, &self.signature, TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1)
    }
}

// A tree with at most 262,145 leaves has at most 19 sibling levels.
pub(crate) const EOG_SIBLINGS_MAX: usize = 19;
fn invalid(why: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.into())
}

fn read_vector<T: BorshDeserialize>(r: &mut impl Read, maximum: usize) -> io::Result<Vec<T>> {
    let len = u32::deserialize_reader(r)? as usize;
    if len > maximum {
        return Err(invalid("tokenizer registration vector exceeds its native wire limit"));
    }
    let mut values = Vec::new();
    values.try_reserve_exact(len).map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "tokenizer registration vector"))?;
    for _ in 0..len {
        values.push(T::deserialize_reader(r)?);
    }
    Ok(values)
}

impl BorshDeserialize for TokenizerRegistrationOpenV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let body = TokenizerRegistrationOpenBodyV1::deserialize_reader(r)?;
        body.check_v1().map_err(invalid)?;
        Ok(Self { body, signature: BorshDeserialize::deserialize_reader(r)? })
    }
}

impl BorshDeserialize for TokenizerRegistrationPartV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let body = TokenizerRegistrationPartBodyV1::deserialize_reader(r)?;
        check_scope(body.version, body.network, body.policy, body.opener).map_err(invalid)?;
        let signature = BorshDeserialize::deserialize_reader(r)?;
        let bytes = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, TOKENIZER_REGISTRATION_PART_BYTES_V1)?;
        if bytes.is_empty() {
            return Err(invalid("empty tokenizer registration part"));
        }
        Ok(Self { body, signature, bytes })
    }
}

impl BorshDeserialize for TokenizerRegistrationFinishV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let body = TokenizerRegistrationFinishBodyV1::deserialize_reader(r)?;
        check_scope(body.version, body.network, body.policy, body.opener).map_err(invalid)?;
        let signature = BorshDeserialize::deserialize_reader(r)?;
        let ids = read_vector(r, ROOTED_TOKENIZER_LIMITS_V1.eog_ids as usize)?;
        if ids.is_empty() {
            return Err(invalid("empty tokenizer registration EOG list"));
        }
        let opening = PalwStepOpeningV1 {
            leaf_index: BorshDeserialize::deserialize_reader(r)?,
            leaf_hash: BorshDeserialize::deserialize_reader(r)?,
            siblings: read_vector(r, EOG_SIBLINGS_MAX)?,
        };
        Ok(Self { body, signature, eog: PalwTokenTableEogOpeningV1 { ids, opening } })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationGeometryV1 {
    pub raw_bytes: u64,
    pub part_count: u32,
}

impl TokenizerRegistrationGeometryV1 {
    pub fn new(raw_bytes: u64) -> Result<Self, &'static str> {
        if !(1..=ROOTED_TOKENIZER_LIMITS_V1.raw_bytes).contains(&raw_bytes) {
            return Err("tokenizer registration raw size is outside the native limit");
        }
        let part_count = u32::try_from(raw_bytes.div_ceil(TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64))
            .map_err(|_| "tokenizer registration part count overflow")?;
        Ok(Self { raw_bytes, part_count })
    }

    pub fn part_v1(&self, index: u32) -> Result<(u64, usize), &'static str> {
        let part = TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64;
        let offset = u64::from(index).checked_mul(part).ok_or("tokenizer registration part offset overflow")?;
        if index >= self.part_count || offset >= self.raw_bytes {
            return Err("tokenizer registration part index is out of range");
        }
        Ok((offset, (self.raw_bytes - offset).min(part) as usize))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationPolicyV1 {
    pub version: u16,
    pub max_raw_bytes: u64,
    pub max_vocab: u32,
    pub max_key_events: u64,
    pub max_eog_ids: u32,
    pub max_rendering_bytes: u64,
    pub max_parser_scratch_bytes: u64,
    pub max_carrier_bytes: u64,
    pub max_retained_bytes: u64,
    pub max_retained_total_bytes: u64,
    /// Candidate-block total, including the once-per-block registration sweep.
    pub service_budget: crate::palw_court_service_v1::ServiceCost,
    /// Upper bound on source blocks merged by one accepting block.
    pub maximum_source_blocks: u64,
    pub max_work: u64,
    pub max_pending_per_opener: u32,
    pub max_pending_total: u32,
    pub max_session_daa: u64,
    pub retention_daa: u64,
    /// Outer carrier framing and worst retained raw/metadata footprint.
    pub carrier_overhead_bytes: u64,
    pub retention_copies: u64,
    /// Total charged metadata allowance across all retained copies.
    pub metadata_bytes: u64,
    /// Raw bytes, key events, vocabulary leaves, EOG IDs, rendering bytes,
    /// signatures and hash bytes, in that order.
    pub work_rates: [u64; 7],
    /// Work units per bounded bookkeeping step (a fixed-key index level,
    /// typed row/packet field copy or checked capacity arithmetic).
    pub bookkeeping_work_per_step: u64,
    pub rent_per_carrier_byte: u64,
    pub rent_per_retained_byte_daa: u64,
    pub rent_per_work: u64,
    pub max_registration_charge: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationQuoteV1 {
    pub geometry: TokenizerRegistrationGeometryV1,
    pub parser_scratch_bytes: u64,
    pub open_carrier_bytes: u64,
    pub total_carrier_bytes: u64,
    pub finish_carrier_bytes: u64,
    pub maximum_part_carrier_bytes: u64,
    pub finish_work: u64,
    pub open_work: u64,
    pub maximum_part_work: u64,
    pub finish_workspace_bytes: u64,
    pub open_cost: crate::palw_court_service_v1::ServiceCost,
    pub part_cost: crate::palw_court_service_v1::ServiceCost,
    pub finish_cost: crate::palw_court_service_v1::ServiceCost,
    pub retained_bytes: u64,
    pub retained_byte_daa: u64,
    pub work: u64,
    pub charge: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenizerRegistrationBookkeepingWorkV1 {
    pub open: u64,
    pub part: u64,
    pub finish: u64,
    /// Aggregate registration GC work charged once for the whole candidate.
    pub sweep: u64,
    /// Charge once when rechecking the full authenticated snapshot.
    pub snapshot: u64,
}

fn add(a: u64, b: u64) -> Result<u64, &'static str> {
    a.checked_add(b).ok_or("tokenizer registration quote addition overflow")
}

fn mul(a: u64, b: u64) -> Result<u64, &'static str> {
    a.checked_mul(b).ok_or("tokenizer registration quote multiplication overflow")
}

impl TokenizerRegistrationPolicyV1 {
    pub fn id_v1(&self) -> Result<Hash64, String> {
        self.check_v1()?;
        Ok(self.fingerprint_v1())
    }

    /// Encode even an invalid configuration for identity/debugging. Native
    /// admission uses id_v1(), which checks shape first.
    pub(crate) fn fingerprint_v1(&self) -> Hash64 {
        stream_digest(
            b"misaka-palw/tokenizer-registration/policy/v1",
            &(
                self,
                TOKENIZER_REGISTRATION_NATIVE_PASSES_V1,
                TOKENIZER_REGISTRATION_ROLLBACK_PASSES_V1,
                TOKENIZER_REGISTRATION_FINISH_PARSER_PASSES_V1,
            ),
        )
        .expect("fixed-size tokenizer registration policy serialization")
    }

    pub fn check_v1(&self) -> Result<(), &'static str> {
        let cap = ROOTED_TOKENIZER_LIMITS_V1;
        if self.version != TOKENIZER_REGISTRATION_VERSION_V1
            || !(1..=cap.raw_bytes).contains(&self.max_raw_bytes)
            || !(1..=cap.vocab).contains(&self.max_vocab)
            || !(1..=cap.key_events).contains(&self.max_key_events)
            || !(1..=cap.eog_ids).contains(&self.max_eog_ids)
            || !(1..=cap.total_rendering_bytes).contains(&self.max_rendering_bytes)
            || self.max_retained_bytes > self.max_retained_total_bytes
            || self.max_session_daa > crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA
            || [self.service_budget.bytes, self.service_budget.work, self.service_budget.io, self.service_budget.controls].contains(&0)
            || self.max_pending_per_opener == 0
            || self.max_pending_per_opener > self.max_pending_total
            || self.work_rates.contains(&0)
            || [
                self.bookkeeping_work_per_step,
                self.maximum_source_blocks,
                self.max_parser_scratch_bytes,
                self.max_carrier_bytes,
                self.max_retained_bytes,
                self.max_work,
                self.max_session_daa,
                self.carrier_overhead_bytes,
                self.retention_copies,
                self.metadata_bytes,
                self.retention_daa,
                self.rent_per_carrier_byte,
                self.rent_per_retained_byte_daa,
                self.rent_per_work,
                self.max_registration_charge,
            ]
            .contains(&0)
        {
            return Err("invalid finite tokenizer registration policy");
        }
        if self.metadata_bytes < self.metadata_floor_v1()? {
            return Err("tokenizer registration metadata below canonical retention floor");
        }
        Ok(())
    }

    /// Minimum canonical metadata allowance. The adopted policy supplies any
    /// additional journal framing, storage/index overhead and copy count.
    pub fn metadata_floor_v1(&self) -> Result<u64, &'static str> {
        const HASH: u64 = 64;
        const POINT: u64 = HASH + 3 * 8;
        const BOND: u64 = HASH + 4;
        const SIG: u64 = MLDSA87_SIGNATURE_LEN as u64;
        // Open body, signed Open, duplicated policy and quote, accepted/key
        // context, vector prefixes, and worst Complete status.
        const OPEN: u64 = 2 + 11 * HASH + BOND + 5 * 8 + 4 + 1 + SIG;
        const POLICY: u64 = 2 + 4 * 4 + 30 * 8;
        const QUOTE: u64 = 8 + 4 + 25 * 8;
        const SESSION: u64 = OPEN + POLICY + QUOTE + POINT + HASH + 8 + 4 + 4 + 2 + 1 + POINT + HASH;
        const PART: u64 = 2 + 4 * HASH + BOND + 4 + 8 + SIG + 4 + POINT + 8;
        const RESERVATION: u64 = HASH + BOND + 3 * 8 + QUOTE + 4 + 1 + POINT;
        // Worst per-packet distinct DAA residual, assignment, packet map
        // and immutable slot history. Row/key tags and index framing also
        // count toward the retention charge.
        const PACKET: u64 = (8 + 4 * 8) + (HASH + 8) + (2 * HASH + 6 * 8) + 8;
        const SESSION_KEYS_AND_BOOK_HEADER: u64 = (1 + HASH) + 1 + 1 + 1 + POLICY + 4 * 8 + 5 * 4 + 1;
        const PART_KEY_AND_ROW_TAG: u64 = 1 + HASH + 4 + 1;
        let parts = self.max_raw_bytes.div_ceil(TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64);
        let packets = add(parts, TOKENIZER_REGISTRATION_FINISH_ATTEMPTS_V1)?;
        let one_copy = add(
            add(SESSION + RESERVATION + SESSION_KEYS_AND_BOOK_HEADER, mul(parts, PART + PART_KEY_AND_ROW_TAG + 4)?)?,
            mul(packets, PACKET)?,
        )?;
        mul(one_copy, self.retention_copies)
    }

    /// Fixed operation envelope for the native indexed book. All counts use
    /// policy maxima, including an incoming Open before quota rejection. Each
    /// session has at most P packets and fewer than 2P causal edges. A full
    /// book check scans the whole book, then rechecks the whole capacity map
    /// inside each active session's causal check.
    pub fn bookkeeping_work_v1(&self) -> Result<TokenizerRegistrationBookkeepingWorkV1, &'static str> {
        self.check_v1()?;
        let parts = self.max_raw_bytes.div_ceil(TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64);
        let per_open = add(parts, TOKENIZER_REGISTRATION_FINISH_ATTEMPTS_V1)?;
        let sessions = add(u64::from(self.max_pending_total), 1)?;
        let packets = mul(sessions, per_open)?;
        // The fixed-key map/ordered-sort unit scales with cardinality, not
        // machine timing. The adopted rate must cover per-level comparisons,
        // node visitation, fixed-size copies, and bounded native ID hashing.
        let levels = u64::from(u64::BITS - packets.leading_zeros());
        let scale = |steps| mul(mul(steps, levels)?, self.bookkeeping_work_per_step);

        // Six construction ownership walks of the book plus session copies,
        // plus the final row/delta and fixed-size policy/quote checks.
        let ownership = mul(16, add(add(packets, sessions)?, per_open)?)?;
        // Residual map validation + session graph generation, per-graph
        // whole-book capacity validation, and up to two predecessor scans.
        let full_check = mul(16, add(packets, mul(sessions, add(packets, per_open)?)?)?)?;
        // Transactional clone, packet detachment, residual reconstruction,
        // and one book check each before and after release/expiry.
        let release = add(mul(2, full_check)?, mul(16, add(packets, per_open)?)?)?;
        // Reserve checks the outer book before and after, then the causal
        // candidate checks and scans its residual map. First-fit can probe
        // every DAA of the full allowed interval for every incoming packet.
        let search = mul(per_open, self.max_session_daa)?;
        let open = add(add(ownership, mul(4, full_check)?)?, add(search, mul(2, release)?)?)?;
        let part = add(ownership, mul(16, per_open)?)?;
        // Completion can supersede all sessions, releasing each individually.
        // A sweep can perform a release or expiry for every retained session.
        let finish = add(ownership, mul(sessions, release)?)?;
        let sweep = add(ownership, mul(sessions, add(release, per_open)?)?)?;
        // Snapshot work also rechecks each indexed Part against its stored
        // bytes. Charge full raw hash bytes plus per-row canonical overhead.
        let snapshot_book = scale(add(full_check, ownership)?)?;
        let retained = mul(sessions, add(mul(self.max_raw_bytes, self.retention_copies)?, self.metadata_bytes)?)?;
        let material_work = mul(mul(retained, 2)?, self.work_rates[6])?;
        // Undo compares and moves journaled rows and restores their indices.
        // One full forward book envelope also bounds one inverse pass,
        // including book equality and reversal of all superseded sessions.
        let carrier_passes = TOKENIZER_REGISTRATION_NATIVE_PASSES_V1 + TOKENIZER_REGISTRATION_ROLLBACK_PASSES_V1;
        Ok(TokenizerRegistrationBookkeepingWorkV1 {
            open: mul(scale(open)?, carrier_passes)?,
            part: mul(scale(part)?, carrier_passes)?,
            finish: mul(scale(finish)?, carrier_passes)?,
            sweep: mul(add(scale(sweep)?, material_work)?, TOKENIZER_REGISTRATION_NATIVE_PASSES_V1)?,
            snapshot: add(snapshot_book, material_work)?,
        })
    }

    /// Independent source-body partition. Keep the accepting candidate's
    /// aggregate sweep envelope outside the partitions.
    pub fn source_budget_v1(&self) -> Result<ServiceCost, &'static str> {
        let budget = self.service_budget;
        let sweep = self.bookkeeping_work_v1()?.sweep;
        let work = budget.work.checked_sub(sweep).ok_or("tokenizer sweep exceeds the total work budget")?;
        let sources = self.maximum_source_blocks;
        let budget = ServiceCost {
            bytes: budget.bytes / sources,
            work: work / sources,
            io: budget.io / sources,
            controls: budget.controls / sources,
        };
        if [budget.bytes, budget.work, budget.io, budget.controls].contains(&0) {
            return Err("tokenizer source partition cannot carry a reservation");
        }
        Ok(budget)
    }

    /// Reserve against one source body so the complete slot can be supplied
    /// without depending on multiple sources being merged at the same DAA.
    pub fn effective_reservation_budget_v1(&self) -> Result<ServiceCost, &'static str> {
        self.source_budget_v1()
    }

    fn repeated_authentication_work_v1(&self, hash_bytes: u64, eog_ids: u64) -> Result<u64, &'static str> {
        let per_pass = add(add(self.work_rates[5], mul(hash_bytes, self.work_rates[6])?)?, mul(eog_ids, self.work_rates[3])?)?;
        mul(per_pass, TOKENIZER_REGISTRATION_NATIVE_PASSES_V1)
    }

    pub(crate) fn open_work_v1(&self, carrier_bytes: u64) -> Result<u64, &'static str> {
        // The repeated Open ID computations cover only its fixed signed body.
        // Full carrier multiples also cover context/domain and fee-scope hashes.
        add(self.repeated_authentication_work_v1(mul(carrier_bytes, 4)?, 0)?, self.bookkeeping_work_v1()?.open)
    }

    pub(crate) fn part_work_v1(&self, carrier_bytes: u64) -> Result<u64, &'static str> {
        // Scope, signature, retained-row verification and duplicate checking
        // can each touch the complete signed payload.
        add(self.repeated_authentication_work_v1(mul(carrier_bytes, 4)?, 0)?, self.bookkeeping_work_v1()?.part)
    }

    fn finish_cost_v1(&self, raw: u64, vocab: u32, declared_key_events: u64) -> Result<ServiceCost, &'static str> {
        let framing = add(self.carrier_overhead_bytes, 1)?;
        let eog = u64::from(self.max_eog_ids);
        let sig = MLDSA87_SIGNATURE_LEN as u64;
        let bytes = add(framing, add(2 + 4 * 64 + 68 + 8 + sig + 4 + 8 + 64 + 4 + EOG_SIBLINGS_MAX as u64 * 64, mul(4, eog)?)?)?;
        // A refused input can build its DOM before counting key events and
        // materialize one last raw-bounded token beyond the rendering limit.
        let parser_quantities = [
            raw,
            raw.max(declared_key_events),
            u64::from(vocab),
            eog,
            add(self.max_rendering_bytes, raw)?,
            0, // The signature and opening are rechecked outside the cache.
            add(mul(raw, 2)?, self.max_rendering_bytes)?,
        ];
        let parser =
            parser_quantities.into_iter().zip(self.work_rates).try_fold(0, |sum, (quantity, rate)| add(sum, mul(quantity, rate)?))?;
        let parser = mul(parser, TOKENIZER_REGISTRATION_FINISH_PARSER_PASSES_V1)?;
        // Every pass verifies the signed Finish and EOG path and rehashes all
        // retained parent Parts before using the memoized parser result.
        // One Finish carrier bounds the combined fixed signed Open and Part
        // bodies plus domain/length framing for each retained Part.
        let part_metadata = mul(raw.div_ceil(TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64), bytes)?;
        let hashes = add(mul(bytes, 4)?, add(raw, part_metadata)?)?;
        let authentication = self.repeated_authentication_work_v1(hashes, eog)?;
        let work = add(add(parser, authentication)?, self.bookkeeping_work_v1()?.finish)?;
        Ok(ServiceCost { bytes, work, io: add(raw, self.metadata_bytes)?, controls: 1 })
    }

    /// Root-independent physical Finish bound, derived by the same monotone
    /// formula as an accepted Open's exact Finish quote.
    pub fn maximum_finish_cost_v1(&self) -> Result<ServiceCost, &'static str> {
        self.finish_cost_v1(self.max_raw_bytes, self.max_vocab, self.max_key_events)
    }

    pub(crate) fn check_source_budget_v1(&self) -> Result<(), &'static str> {
        let framing = add(self.carrier_overhead_bytes, 1)?;
        let sig = MLDSA87_SIGNATURE_LEN as u64;
        let open_bytes = add(framing, 2 + 11 * 64 + 68 + 5 * 8 + 4 + 1 + sig)?;
        let raw = self.max_raw_bytes.min(TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64);
        let part_bytes = add(add(framing, 2 + 4 * 64 + 68 + 4 + 8 + sig + 4)?, raw)?;
        let open = ServiceCost { bytes: open_bytes, work: self.open_work_v1(open_bytes)?, io: self.metadata_bytes, controls: 1 };
        let part = ServiceCost {
            bytes: part_bytes,
            work: self.part_work_v1(part_bytes)?,
            io: add(mul(raw, self.retention_copies)?, self.metadata_bytes)?,
            controls: 1,
        };
        let finish = self.maximum_finish_cost_v1()?;
        let budget = self.source_budget_v1()?;
        if [open, part, finish].into_iter().any(|cost| {
            cost.bytes > self.max_carrier_bytes
                || cost.bytes > budget.bytes
                || cost.work > budget.work
                || cost.io > budget.io
                || cost.controls > budget.controls
        }) {
            return Err("tokenizer source partition cannot carry a maximum policy packet");
        }
        Ok(())
    }

    /// Prices the exact signed Open using key-work bounds from accepted class
    /// semantics. Carrier overhead and retained-copy costs are supplied by the
    /// native profile, including outer framing, row, journal and snapshot IO.
    pub fn quote_v1(
        &self,
        open: &TokenizerRegistrationOpenBodyV1,
        accepted_daa: u64,
        declared_key_events: u64,
    ) -> Result<TokenizerRegistrationQuoteV1, String> {
        open.check_v1()?;
        let raw = open.statement.exact_raw_bytes;
        if open.policy != self.id_v1()?
            || raw > self.max_raw_bytes
            || open.statement.vocab > self.max_vocab
            || !(1..=self.max_key_events).contains(&declared_key_events)
            || accepted_daa >= open.accept_until_daa
            || open.finish_until_daa.checked_sub(accepted_daa).is_none_or(|span| span > self.max_session_daa)
            || open.finish_until_daa.checked_add(self.retention_daa) != Some(open.material_until_daa)
        {
            return Err("tokenizer registration open exceeds policy or changes its fixed clocks".into());
        }
        let geometry = TokenizerRegistrationGeometryV1::new(raw)?;
        let count = u64::from(geometry.part_count);
        let sig = MLDSA87_SIGNATURE_LEN as u64;
        // Enum tag and outer framing, followed by the canonical signed body.
        let framing = add(self.carrier_overhead_bytes, 1)?;
        let open_carrier_bytes = add(add(borsh::object_length(open).map_err(|err| err.to_string())? as u64, sig)?, framing)?;
        let part_overhead = add(framing, 2 + 4 * 64 + 68 + 4 + 8 + sig + 4)?;
        let eog = u64::from(self.max_eog_ids);
        let finish_cost = self.finish_cost_v1(raw, open.statement.vocab, declared_key_events)?;
        let finish_carrier_bytes = finish_cost.bytes;
        let finish_work = finish_cost.work;
        let part_max = add(part_overhead, raw.min(TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64))?;
        let total_carrier_bytes = add(
            add(open_carrier_bytes, mul(count, part_max)?)?,
            mul(TOKENIZER_REGISTRATION_FINISH_ATTEMPTS_V1, finish_carrier_bytes)?,
        )?;
        let parser_scratch_bytes = add(add(mul(raw, 128)?, mul(eog, 4)?)?, 16_384)?;
        let retained_bytes = add(mul(raw, self.retention_copies)?, self.metadata_bytes)?;
        let retained_byte_daa = mul(retained_bytes, open.material_until_daa - accepted_daa)?;
        let attempts = TOKENIZER_REGISTRATION_FINISH_ATTEMPTS_V1;
        let finish_workspace_bytes = add(add(parser_scratch_bytes, raw)?, finish_carrier_bytes)?;
        let open_work = self.open_work_v1(open_carrier_bytes)?;
        let maximum_part_work = self.part_work_v1(part_max)?;
        let open_cost = ServiceCost { bytes: open_carrier_bytes, work: open_work, io: self.metadata_bytes, controls: 1 };
        let part_cost = ServiceCost {
            bytes: part_max,
            work: maximum_part_work,
            io: add(mul(raw.min(TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64), self.retention_copies)?, self.metadata_bytes)?,
            controls: 1,
        };
        // The actual book reserves the full part profile at every part slot,
        // including the short tail. Billing work uses the same upper bound.
        let work = add(add(open_work, mul(count, maximum_part_work)?)?, mul(attempts, finish_work)?)?;
        let budget = self.effective_reservation_budget_v1()?;
        if [open_cost, part_cost, finish_cost]
            .iter()
            .any(|cost| cost.bytes > budget.bytes || cost.work > budget.work || cost.io > budget.io || cost.controls > budget.controls)
        {
            return Err("tokenizer registration packet exceeds native service capacity".into());
        }
        let charge = add(
            add(mul(total_carrier_bytes, self.rent_per_carrier_byte)?, mul(retained_byte_daa, self.rent_per_retained_byte_daa)?)?,
            mul(work, self.rent_per_work)?,
        )?;
        if open_carrier_bytes.max(part_max).max(finish_carrier_bytes) > self.max_carrier_bytes
            || finish_workspace_bytes > self.max_parser_scratch_bytes
            || retained_bytes > self.max_retained_bytes
            || work > self.max_work
            || charge > self.max_registration_charge.min(open.max_registration_charge)
        {
            return Err("tokenizer registration quote exceeds native resource or signed charge ceiling".into());
        }
        Ok(TokenizerRegistrationQuoteV1 {
            geometry,
            parser_scratch_bytes,
            open_carrier_bytes,
            total_carrier_bytes,
            finish_carrier_bytes,
            maximum_part_carrier_bytes: part_max,
            finish_work,
            open_work,
            maximum_part_work,
            finish_workspace_bytes,
            open_cost,
            part_cost,
            finish_cost,
            retained_bytes,
            retained_byte_daa,
            work,
            charge,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn finish_work_rehashes_parent_parts_in_every_pass_but_parses_once() {
        let (policy, _) = fixture();
        // Keep both raw lengths in one Part; the fixed transport/opening
        // geometry and policy book bounds cancel when comparing quotes.
        let shorter = policy.finish_cost_v1(60_000, 128, 10).unwrap();
        let longer = policy.finish_cost_v1(60_001, 128, 10).unwrap();
        let parser_per_byte = 1 + 1 + 1 + 2;
        let repeated_part_hash_per_byte = 1;
        assert_eq!(
            longer.work - shorter.work,
            parser_per_byte * TOKENIZER_REGISTRATION_FINISH_PARSER_PASSES_V1
                + repeated_part_hash_per_byte * TOKENIZER_REGISTRATION_NATIVE_PASSES_V1
        );
        assert_ne!(policy.id_v1().unwrap(), stream_digest(b"misaka-palw/tokenizer-registration/policy/v1", &policy).unwrap());
    }

    pub(crate) fn fixture() -> (TokenizerRegistrationPolicyV1, TokenizerRegistrationOpenBodyV1) {
        let h = Hash64::from_u64_word;
        let policy = TokenizerRegistrationPolicyV1 {
            version: 1,
            max_raw_bytes: 131_072,
            max_vocab: 1_024,
            max_key_events: 200_000,
            max_eog_ids: 8,
            max_rendering_bytes: 131_072,
            max_parser_scratch_bytes: 32_000_000,
            max_carrier_bytes: 80_000,
            max_retained_bytes: 1_000_000,
            max_retained_total_bytes: 8_000_000,
            maximum_source_blocks: 1,
            // Scale the local single-pass fixture by the full forward and inverse bound.
            service_budget: ServiceCost { bytes: 100_000, work: 7 * 12_000_000, io: 1_000_000, controls: 2 },
            max_work: 7 * 10_000_000,
            max_pending_per_opener: 2,
            max_pending_total: 8,
            max_session_daa: 100,
            retention_daa: 20,
            carrier_overhead_bytes: 5,
            retention_copies: 3,
            metadata_bytes: 64_000,
            work_rates: [1; 7],
            bookkeeping_work_per_step: 1,
            rent_per_carrier_byte: 1,
            rent_per_retained_byte_daa: 1,
            rent_per_work: 1,
            max_registration_charge: 100_000_000,
        };
        let body = TokenizerRegistrationOpenBodyV1 {
            version: 1,
            network: h(1),
            policy: policy.id_v1().unwrap(),
            pins: PalwTfp1TokenizerIntakePinsV2 { class_id: h(2), container_root: h(3), semantics_id: h(4), inventory_id: h(5) },
            artifact_root: h(6),
            statement: PalwTokenizerRenderingStatementV2 {
                tokenizer_id: h(7),
                exact_raw_bytes: 65_537,
                table_root: h(8),
                vocab: 1_024,
                rule: crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
            },
            eog_digest: h(9),
            opener: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(10), 1)),
            nonce: h(11),
            accept_until_daa: 40,
            finish_until_daa: 80,
            material_until_daa: 100,
            max_registration_charge: 100_000_000,
        };
        (policy, body)
    }

    #[test]
    fn quote_pins_indexed_parts_and_fixed_retention() {
        let (policy, mut body) = fixture();
        let quote = policy.quote_v1(&body, 20, 10_000).unwrap();
        assert_eq!(quote.geometry.part_v1(0).unwrap(), (0, 65_536));
        assert_eq!(quote.geometry.part_v1(1).unwrap(), (65_536, 1));
        assert!(quote.geometry.part_v1(2).is_err());
        assert_eq!(quote.retained_byte_daa, quote.retained_bytes * 80);
        assert_eq!(quote.work, quote.open_cost.work + 2 * quote.part_cost.work + 2 * quote.finish_cost.work);
        assert_eq!(quote.total_carrier_bytes, quote.open_cost.bytes + 2 * quote.part_cost.bytes + 2 * quote.finish_cost.bytes);
        assert!(policy.quote_v1(&body, 40, 10_000).is_err());
        body.max_registration_charge = quote.charge - 1;
        assert!(policy.quote_v1(&body, 20, 10_000).is_err());
        body.max_registration_charge = quote.charge;
        body.material_until_daa -= 1;
        assert!(policy.quote_v1(&body, 20, 10_000).is_err());
    }

    #[test]
    fn bookkeeping_bounds_scale_and_reject_underfunded_metadata() {
        let (policy, body) = fixture();
        let base = policy.bookkeeping_work_v1().unwrap();
        let longer_window = TokenizerRegistrationPolicyV1 { max_session_daa: policy.max_session_daa + 1, ..policy };
        assert!(longer_window.bookkeeping_work_v1().unwrap().open > base.open);
        let larger_book = TokenizerRegistrationPolicyV1 { max_pending_total: policy.max_pending_total + 1, ..policy };
        let larger = larger_book.bookkeeping_work_v1().unwrap();
        assert!(larger.part > base.part && larger.finish > base.finish);
        assert!(larger.sweep > base.sweep && larger.snapshot > base.snapshot);

        let floor = policy.metadata_floor_v1().unwrap();
        let insufficient = TokenizerRegistrationPolicyV1 { metadata_bytes: floor - 1, ..policy };
        assert!(insufficient.id_v1().is_err());
        let at_floor = TokenizerRegistrationPolicyV1 { metadata_bytes: floor, ..policy };
        assert!(at_floor.id_v1().is_ok());
        let quote = policy.quote_v1(&body, 20, 10_000).unwrap();
        assert_eq!(quote.open_cost.work, policy.open_work_v1(quote.open_cost.bytes).unwrap());
        assert_eq!(quote.part_cost.work, policy.part_work_v1(quote.part_cost.bytes).unwrap());
        assert!(quote.finish_cost.work > base.finish);
    }

    #[test]
    fn invalid_tokenizer_input_is_priced_before_a_small_declared_limit_can_refuse_it() {
        let (mut policy, mut body) = fixture();
        policy.max_rendering_bytes = 1;
        body.policy = policy.id_v1().unwrap();
        let quote = policy.quote_v1(&body, 20, 1).unwrap();
        let raw = body.statement.exact_raw_bytes;
        // A failing DOM parse can process raw-bounded keys before the
        // declaration is checked and materialize one token beyond the cap.
        let work_floor =
            raw * policy.work_rates[0] + raw * policy.work_rates[1] + (raw + policy.max_rendering_bytes) * policy.work_rates[4];
        assert!(quote.finish_cost.work >= work_floor);
        assert!(quote.work >= TOKENIZER_REGISTRATION_FINISH_ATTEMPTS_V1 * work_floor);
        policy.service_budget.work = policy.bookkeeping_work_v1().unwrap().sweep + quote.finish_cost.work - 1;
        body.policy = policy.id_v1().unwrap();
        assert!(policy.quote_v1(&body, 20, 1).is_err());
    }

    #[test]
    fn signed_open_and_bounded_wire_are_scope_bound() {
        let (_, body) = fixture();
        let mut open = TokenizerRegistrationOpenV1 { body, signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN] };
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([121; 32]);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            open.signing_message_v1().unwrap().as_byte_slice(),
            TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1,
            [122; 32],
        )
        .unwrap();
        open.signature.copy_from_slice(signature.as_ref());
        let public_key = key.verification_key.as_ref();
        open.verify_signature_v1(public_key).unwrap();
        let wire = TokenizerRegistrationWireV1::Open(open.clone());
        let bytes = borsh::to_vec(&wire).unwrap();
        assert_eq!(borsh::from_slice::<TokenizerRegistrationWireV1>(&bytes).unwrap(), wire);
        assert!(borsh::from_slice::<TokenizerRegistrationWireV1>(&bytes[..bytes.len() - 1]).is_err());
        let mut unknown = bytes;
        unknown[1] = 2;
        assert!(borsh::from_slice::<TokenizerRegistrationWireV1>(&unknown).is_err());
        open.body.network = Hash64::from_u64_word(42);
        assert!(open.verify_signature_v1(public_key).is_err());

        let part = TokenizerRegistrationPartV1 {
            body: TokenizerRegistrationPartBodyV1 {
                version: 1,
                network: body.network,
                policy: body.policy,
                open_id: body.id_v1().unwrap(),
                opener: body.opener,
                index: 1,
                offset: 65_536,
                payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(&[1]).unwrap(),
            },
            signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            bytes: vec![1],
        };
        part.check_open_v1(&body).unwrap();
        let mut bytes = borsh::to_vec(&TokenizerRegistrationWireV1::Part(part)).unwrap();
        let length_at = bytes.len() - 5;
        bytes[length_at..length_at + 4].copy_from_slice(&65_537u32.to_le_bytes());
        assert!(borsh::from_slice::<TokenizerRegistrationWireV1>(&bytes).is_err());
    }
    #[test]
    fn finish_reuses_registered_eog_opening_and_rejects_oversized_lists() {
        let (policy, mut open) = fixture();
        let table = crate::palw_fp_constraint_job_v1::PalwTokenTableV1 {
            entries: vec![Some(vec![b'a']), Some(vec![b'b'])],
            eog_token_ids: vec![1],
            tokenizer_id: open.statement.tokenizer_id,
        };
        let eog = table.eog_opening().unwrap();
        open.statement.vocab = table.vocab();
        open.statement.table_root = table.root();
        open.eog_digest = TokenizerRegistrationFinishV1::eog_digest_v1(&eog).unwrap();
        let mut finish = TokenizerRegistrationFinishV1 {
            body: TokenizerRegistrationFinishBodyV1 {
                version: 1,
                network: open.network,
                policy: open.policy,
                open_id: open.id_v1().unwrap(),
                opener: open.opener,
                eog_digest: open.eog_digest,
                finish_until_daa: open.finish_until_daa,
            },
            signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            eog,
        };
        finish.check_open_v1(&open, &policy).unwrap();
        let wire = TokenizerRegistrationWireV1::Finish(finish.clone());
        let bytes = borsh::to_vec(&wire).unwrap();
        assert_eq!(borsh::from_slice::<TokenizerRegistrationWireV1>(&bytes).unwrap(), wire);
        assert!(borsh::from_slice::<TokenizerRegistrationWireV1>(&bytes[..bytes.len() - 1]).is_err());
        finish.eog.ids.resize(4_097, 1);
        let oversized = borsh::to_vec(&TokenizerRegistrationWireV1::Finish(finish)).unwrap();
        assert!(borsh::from_slice::<TokenizerRegistrationWireV1>(&oversized).is_err());
    }
}
