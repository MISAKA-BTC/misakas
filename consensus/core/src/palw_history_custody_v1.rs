//! Prospective custody of the exact retained accepted history, not peer availability.
//! The native fold must authenticate the initial head and the signed custody grant.
//! Recovery messages never enter the semantic history they are recovering.
use crate::{
    Hash64,
    palw_prefix_history_v1::{PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1, PalwPrefixHistoryRecordV1, PalwPrefixHistoryRequestV1},
    palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwRenderingResourcePricesV1},
    palw_state_v2::PalwBondKeyV2,
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PALW_HISTORY_CUSTODY_PART_BYTES_V1: usize = 32_768;
pub const PALW_HISTORY_PLEDGE_CONTEXT_V1: &[u8] = b"misaka-palw/history-custody/pledge/v1/mldsa87";
pub const PALW_HISTORY_GRANT_CONTEXT_V1: &[u8] = b"misaka-palw/history-custody/valid-grant/v1/mldsa87";
pub const PALW_HISTORY_FP_ENVELOPE_MAGIC_V1: &[u8; 4] = b"HFP1";
pub const PALW_HISTORY_FP_ENVELOPE_MAX_BYTES_V1: usize = crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES + 8192;

/// An explicit outer envelope; no trailing bytes are ignored by a legacy decoder.
/// The original inner FP claim, PoW message and signature are byte-for-byte intact.
#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwHistoryFpEnvelopeV1 {
    pub version: u16,
    pub original_payload: Vec<u8>,
    pub pledge: PalwHistoryCustodyPledgeV1,
}
impl PalwHistoryFpEnvelopeV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        self.pledge.check_v1()?;
        if self.version != 1
            || self.original_payload.is_empty()
            || self.original_payload.len() > crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES
            || self.original_payload.starts_with(PALW_HISTORY_FP_ENVELOPE_MAGIC_V1)
            || borsh::object_length(self).map_err(|e| e.to_string())? + PALW_HISTORY_FP_ENVELOPE_MAGIC_V1.len()
                > PALW_HISTORY_FP_ENVELOPE_MAX_BYTES_V1
        {
            return Err("history FP envelope is oversized, recursive or not its prospective version".into());
        }
        Ok(())
    }
    pub fn encode_v1(&self) -> Result<Vec<u8>, String> {
        self.check_v1()?;
        let mut bytes = PALW_HISTORY_FP_ENVELOPE_MAGIC_V1.to_vec();
        bytes.extend(borsh::to_vec(self).map_err(|e| e.to_string())?);
        Ok(bytes)
    }
    pub fn decode_v1(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PALW_HISTORY_FP_ENVELOPE_MAX_BYTES_V1 {
            return Err("history FP envelope exceeds bounded wire admission".into());
        }
        let body = bytes.strip_prefix(PALW_HISTORY_FP_ENVELOPE_MAGIC_V1).ok_or("history FP envelope magic is absent")?;
        let envelope: Self = borsh::from_slice(body).map_err(|e| e.to_string())?;
        envelope.check_v1()?;
        Ok(envelope)
    }
    pub fn original_fp_v1(&self) -> Result<crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3, String> {
        self.check_v1()?;
        let payload: crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 =
            borsh::from_slice(&self.original_payload).map_err(|e| e.to_string())?;
        let c = &payload.commitment;
        if self.pledge.claim != payload.claim_id()
            || self.pledge.producer != PalwBondKeyV2(c.job.executor_bond)
            || self.pledge.execution_root != c.execution_root
            || self.pledge.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_v1(c)
        {
            return Err("history pledge changes the exact original FP claim, producer, execution or job".into());
        }
        Ok(payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwHistoryCustodyPledgeV1 {
    pub version: u16,
    pub claim: Hash64,
    pub producer: PalwBondKeyV2,
    pub execution_root: Hash64,
    pub job_identity: Hash64,
    pub cu_ruleset_id: Hash64,
    /// Native recomputes this from authenticated inventory and required-family SUM.
    pub resource_plan_id: Hash64,
    pub maximum_records: u64,
    pub maximum_custodians: u64,
    pub accept_until_daa: u64,
    pub material_until_daa: u64,
    pub signature: Vec<u8>,
}
impl PalwHistoryCustodyPledgeV1 {
    pub fn authenticate_original_object_v1<F>(
        &self,
        object: &crate::palw_state_v2::PalwConsensusObjectV2,
        verify: F,
    ) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        let crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
            claim,
            bond,
            execution_root,
            job_pin,
            executor_pubkey,
            ..
        } = object
        else {
            return Err("history pledge is not paired with an original FP-family commitment".into());
        };
        if (self.claim, self.producer, self.execution_root, self.job_identity) != (*claim, *bond, *execution_root, *job_pin) {
            return Err("history pledge changes its exact transaction-local original commitment".into());
        }
        self.validate_signature_v1(executor_pubkey, verify)
    }
    pub fn check_v1(&self) -> Result<(), String> {
        if self.version != 1
            || self.claim == Hash64::default()
            || self.execution_root == Hash64::default()
            || self.job_identity == Hash64::default()
            || self.cu_ruleset_id == Hash64::default()
            || self.resource_plan_id == Hash64::default()
            || self.maximum_records == 0
            || self.maximum_custodians == 0
            || self.accept_until_daa >= self.material_until_daa
            || self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
        {
            return Err("history custody pledge lacks its exact original claim, signed terms or retention horizon".into());
        }
        Ok(())
    }
    pub fn pledge_id_v1(&self) -> Hash64 {
        let raw = borsh::to_vec(&(
            self.version,
            self.claim,
            self.producer,
            self.execution_root,
            self.job_identity,
            self.cu_ruleset_id,
            self.resource_plan_id,
            self.maximum_records,
            self.maximum_custodians,
            self.accept_until_daa,
            self.material_until_daa,
        ))
        .expect("fixed history pledge serializes");
        Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/history-custody/pledge-id/v1")
                .hash(&raw)
                .as_bytes()
                .try_into()
                .expect("64"),
        )
    }
    /// The caller resolves the exact original producer bond key, not a carried key.
    pub fn validate_signature_v1<F>(&self, public_key: &[u8], verify: F) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.check_v1()?;
        if public_key.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || !verify(public_key, self.pledge_id_v1().as_byte_slice(), &self.signature, PALW_HISTORY_PLEDGE_CONTEXT_V1)
        {
            return Err("history custody pledge signature does not verify under the original producer bond".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwHistoryCustodyGrantV1 {
    pub version: u16,
    pub claim: Hash64,
    pub seat: PalwBondKeyV2,
    pub signed_daa: u64,
    pub pledge_id: Hash64,
    pub valid_receipt_digest: Hash64,
    pub signature: Vec<u8>,
}
impl PalwHistoryCustodyGrantV1 {
    pub fn receipt_digest_v1(receipt: &crate::palw_panel_v2::PalwSeatReceiptV2) -> Hash64 {
        let raw = borsh::to_vec(receipt).expect("fixed signed seat receipt serializes");
        Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/history-custody/valid-receipt/v1")
                .hash(&raw)
                .as_bytes()
                .try_into()
                .expect("64"),
        )
    }
    pub fn check_receipt_v1(&self, receipt: &crate::palw_panel_v2::PalwSeatReceiptV2) -> Result<(), String> {
        if self.version != 1
            || self.pledge_id == Hash64::default()
            || self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || (self.claim, self.seat, self.signed_daa) != (receipt.claim, receipt.seat_bond, receipt.signed_daa)
            || receipt.verdict != crate::palw_panel_v2::PalwReceiptVerdictV2::Valid
            || self.valid_receipt_digest != Self::receipt_digest_v1(receipt)
        {
            return Err("history grant does not bind the exact original signed Valid receipt".into());
        }
        Ok(())
    }
    pub fn signing_message_v1(&self) -> Hash64 {
        let raw = borsh::to_vec(&(self.version, self.claim, self.seat, self.signed_daa, self.pledge_id, self.valid_receipt_digest))
            .expect("fixed history grant serializes");
        Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/history-custody/valid-grant-id/v1")
                .hash(&raw)
                .as_bytes()
                .try_into()
                .expect("64"),
        )
    }
    pub fn validate_signature_v1<F>(
        &self,
        receipt: &crate::palw_panel_v2::PalwSeatReceiptV2,
        public_key: &[u8],
        verify: F,
    ) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.check_receipt_v1(receipt)?;
        if public_key.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || !verify(public_key, self.signing_message_v1().as_byte_slice(), &self.signature, PALW_HISTORY_GRANT_CONTEXT_V1)
        {
            return Err("history custody grant signature does not verify under the original credited seat bond".into());
        }
        Ok(())
    }
}

/// Accepted only under the prospective bundle fence. Existing semantic-source
/// custody does not imply an obligation to retain a later owner's signed log.
#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwHistoryCustodyContractV1 {
    pub pledge: PalwHistoryCustodyPledgeV1,
    pub producer: PalwBondKeyV2,
    pub maximum_records: u64,
    pub maximum_custodians: u64,
    pub custody: crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1,
}
impl PalwHistoryCustodyContractV1 {
    pub fn check_v1(&self, held_panel: &BTreeSet<PalwBondKeyV2>) -> Result<(), String> {
        self.custody.check(held_panel)?;
        self.pledge.check_v1()?;
        if self.maximum_records == 0
            || self.maximum_custodians == 0
            || u64::try_from(self.custody.expected_panel.len())
                .map_err(|_| "history custody count overflow")?
                .checked_add(1)
                .is_none_or(|n| n > self.maximum_custodians)
            || self.custody.expected_panel.contains(&self.producer)
            || (
                self.producer,
                self.maximum_records,
                self.maximum_custodians,
                self.custody.execution_root,
                self.custody.job_identity,
                self.custody.cu_ruleset_id,
            ) != (
                self.pledge.producer,
                self.pledge.maximum_records,
                self.pledge.maximum_custodians,
                self.pledge.execution_root,
                self.pledge.job_identity,
                self.pledge.cu_ruleset_id,
            )
            || self.custody.accepted_daa >= self.pledge.accept_until_daa
        {
            return Err("history custody differs from its admitted native panel envelope".into());
        }
        Ok(())
    }
    pub fn commitment_v1(&self) -> Result<Hash64, String> {
        self.pledge.check_v1()?;
        Ok(self.pledge.pledge_id_v1())
    }
    /// Caller verifies the receipt's original signature and its explicit history
    /// contract commitment before crediting it here. Membership alone never credits.
    pub fn qualify_v1<F>(
        &mut self,
        claim: Hash64,
        receipt: &crate::palw_panel_v2::PalwSeatReceiptV2,
        grant: &PalwHistoryCustodyGrantV1,
        now: u64,
        public_key: &[u8],
        verify: F,
    ) -> bool
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        if claim != self.pledge.claim
            || self.commitment_v1().ok() != Some(grant.pledge_id)
            || grant.validate_signature_v1(receipt, public_key, verify).is_err()
            || !self.custody.qualify(claim, receipt, now)
        {
            return false;
        }
        true
    }
    pub fn current_signed_custodians_v1(&self) -> BTreeSet<PalwBondKeyV2> {
        self.custody.current_signed_v1().expected_panel.into_iter().chain(std::iter::once(self.producer)).collect()
    }
}

/// This supplements, rather than replaces, the semantic family's work quote.
/// Every literal custodian has protected space and a full bounded answer budget.
pub fn history_recovery_resource_plan_v1(
    maximum_records: u64,
    custodian_count: usize,
    prices: PalwRenderingResourcePricesV1,
) -> Result<PalwRenderingResourcePlanV1, String> {
    if custodian_count == 0 || prices.inclusion_margin_daa == 0 {
        return Err("history recovery quote needs a bounded literal custody set and positive inclusion margin".into());
    }
    let n = u128::from(maximum_records);
    // Include the independent owner, who may answer without acquiring liability.
    let slots = u128::from(u64::try_from(custodian_count).map_err(|_| "history custody count overflows u64")?) + 1;
    let record = PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 as u128;
    let parts = record.div_ceil(PALW_HISTORY_CUSTODY_PART_BYTES_V1 as u128);
    let mul = |a: u128, b: u128| a.checked_mul(b).ok_or_else(|| "history recovery quote multiplication overflows".to_string());
    let add = |a: u128, b: u128| a.checked_add(b).ok_or_else(|| "history recovery quote addition overflows".to_string());
    let bytes = mul(mul(n, slots)?, record)?;
    let moves = add(add(mul(n, add(1, mul(slots, parts + 2)?)?)?, 1)?, u128::from(prices.reorg_replay_moves))?;
    // Borsh state retains one unanswered record per slot, never the whole chain.
    let row = add(mul(slots, record + 512)?, 4096)?;
    // Accepted state, COW next state, reversible old/new checkpoints and allocator
    // capacity may coexist. Use the same conservative eight-row envelope as the
    // semantic custody courts, plus transport/decoded-record scratch.
    let ram = add(mul(row, 8)?, record * 4)?;
    // Accepted reversible deltas can retain a full mutable checkpoint at each
    // bounded move. Pricing only the final live row misses this native disk cost.
    let journals = mul(moves, add(row, 90_000)?)?;
    let archive = mul(n, record + 256)?;
    let storage = add(add(row, journals)?, archive)?;
    let rent = add(mul(bytes, u128::from(prices.rent_per_stream_byte))?, mul(storage, u128::from(prices.rent_per_row_byte))?)?;
    let narrow = |v: u128| u64::try_from(v).map_err(|_| "history recovery resource quote overflows u64".to_string());
    Ok(PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab: 0,
        maximum_stream_bytes: narrow(bytes)?,
        maximum_stream_leaves: maximum_records,
        maximum_eog_bytes: 0,
        maximum_moves: narrow(moves)?,
        maximum_parser_steps: 0,
        maximum_blake_blocks: narrow(mul(mul(n, slots)?, record.div_ceil(128) + 1)?)?,
        inclusion_daa: narrow(mul(moves, u128::from(prices.inclusion_margin_daa))?)?,
        rent_sompi: narrow(rent)?,
        deposit_sompi: narrow(mul(moves, u128::from(prices.deposit_per_move))?)?,
        row_bytes: narrow(storage)?,
        resident_ram_bytes: narrow(ram)?,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub enum PalwHistoryRecoveryActionV1 {
    Demand,
    Begin { answerer: PalwBondKeyV2, total_bytes: u32 },
    Part { answerer: PalwBondKeyV2, offset: u32, bytes: Vec<u8> },
    Finish { answerer: PalwBondKeyV2 },
    Collect,
}
impl PalwHistoryRecoveryActionV1 {
    pub fn answerer_v1(&self) -> Option<PalwBondKeyV2> {
        match self {
            Self::Begin { answerer, .. } | Self::Part { answerer, .. } | Self::Finish { answerer } => Some(*answerer),
            _ => None,
        }
    }
    pub fn check_wire_v1(&self) -> Result<(), String> {
        match self {
            Self::Begin { total_bytes, .. }
                if *total_bytes == 0 || *total_bytes as usize > PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 =>
            {
                Err("history answer exceeds the original bounded record format".into())
            }
            Self::Part { bytes, .. } if bytes.is_empty() || bytes.len() > PALW_HISTORY_CUSTODY_PART_BYTES_V1 => {
                Err("history answer part exceeds the bounded native move".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwHistoryRecoveryBufferV1 {
    pub total_bytes: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwHistoryRecoveryDemandV1 {
    pub request: PalwPrefixHistoryRequestV1,
    pub opened_daa: u64,
    pub answer_until_daa: u64,
    pub buffers: BTreeMap<PalwBondKeyV2, PalwHistoryRecoveryBufferV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwHistoryRecoveryDefaultV1 {
    pub request: PalwPrefixHistoryRequestV1,
    pub opened_daa: u64,
    pub answer_until_daa: u64,
    pub collected_daa: u64,
    /// Exact signed custodians, not every drawn or abstaining panel seat.
    pub custodians: BTreeSet<PalwBondKeyV2>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwHistoryRecoveryEffectV1 {
    Advanced,
    Recovered(PalwPrefixHistoryRecordV1),
    Default(PalwHistoryRecoveryDefaultV1),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwHistoryRecoveryV1 {
    pub initial: PalwPrefixHistoryRequestV1,
    /// Zero sequence/head is the only completed backwards traversal.
    pub remaining_sequence: u64,
    pub remaining_head: Hash64,
    pub opened_daa: u64,
    pub recovery_until_daa: u64,
    pub collect_until_daa: u64,
    pub response_span_daa: u64,
    pub maximum_custodians: u64,
    pub custodians: BTreeSet<PalwBondKeyV2>,
    pub demand: Option<PalwHistoryRecoveryDemandV1>,
    pub default: Option<PalwHistoryRecoveryDefaultV1>,
}
impl PalwHistoryRecoveryV1 {
    /// Caller must prove both the original rooted head and prospective history
    /// custody using accepted state. A signer list from an RPC is not authority.
    pub fn new_v1(
        initial: PalwPrefixHistoryRequestV1,
        opened_daa: u64,
        recovery_until_daa: u64,
        collect_until_daa: u64,
        response_span_daa: u64,
        maximum_custodians: u64,
        custodians: BTreeSet<PalwBondKeyV2>,
    ) -> Result<Self, String> {
        initial.check_v1()?;
        let row = Self {
            remaining_sequence: initial.sequence,
            remaining_head: initial.head,
            initial,
            opened_daa,
            recovery_until_daa,
            collect_until_daa,
            response_span_daa,
            maximum_custodians,
            custodians,
            demand: None,
            default: None,
        };
        row.check_v1()?;
        Ok(row)
    }
    pub fn complete_v1(&self) -> bool {
        self.remaining_sequence == 0 && self.remaining_head == Hash64::default() && self.default.is_none()
    }
    pub fn remaining_request_v1(&self) -> Option<PalwPrefixHistoryRequestV1> {
        (self.remaining_sequence != 0).then(|| PalwPrefixHistoryRequestV1 {
            sequence: self.remaining_sequence,
            head: self.remaining_head,
            ..self.initial.clone()
        })
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.initial.check_v1()?;
        if self.opened_daa < self.initial.reserved_daa
            || self.opened_daa >= self.recovery_until_daa
            || self.recovery_until_daa >= self.collect_until_daa
            || self.collect_until_daa > self.initial.material_until_daa
            || self.response_span_daa == 0
            || self.custodians.is_empty()
            || u64::try_from(self.custodians.len()).map_err(|_| "history custody count overflow")? > self.maximum_custodians
            || self.custodians.contains(&self.initial.owner)
            || self.remaining_sequence > self.initial.sequence
            || (self.remaining_sequence == 0) != (self.remaining_head == Hash64::default())
        {
            return Err("history recovery changes the frozen seed, work, custody or clocks".into());
        }
        if let Some(demand) = &self.demand {
            if Some(&demand.request) != self.remaining_request_v1().as_ref()
                || demand.opened_daa < self.opened_daa
                || demand.opened_daa >= demand.answer_until_daa
                || demand.answer_until_daa > self.recovery_until_daa
                || demand.answer_until_daa.checked_sub(demand.opened_daa) != Some(self.response_span_daa)
                || demand.buffers.iter().any(|(bond, buffer)| {
                    (!self.custodians.contains(bond) && *bond != self.initial.owner)
                        || buffer.total_bytes == 0
                        || buffer.total_bytes as usize > PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1
                        || buffer.bytes.len() > buffer.total_bytes as usize
                })
            {
                return Err("history demand or protected buffer differs from the accepted backwards cursor".into());
            }
        }
        if let Some(default) = &self.default {
            if self.demand.is_some()
                || Some(&default.request) != self.remaining_request_v1().as_ref()
                || default.custodians != self.custodians
                || default.opened_daa < self.opened_daa
                || default.answer_until_daa.checked_sub(default.opened_daa) != Some(self.response_span_daa)
                || default.answer_until_daa > self.recovery_until_daa
                || default.collected_daa < default.answer_until_daa
                || default.collected_daa >= self.collect_until_daa
            {
                return Err("history default is not the exact expired accepted demand".into());
            }
        }
        Ok(())
    }
    /// Authorization is verified by the native carriage against action.answerer
    /// (or the exact owner for Demand/Collect), before this pure transition runs.
    pub fn apply_v1(&mut self, action: &PalwHistoryRecoveryActionV1, now: u64) -> Result<PalwHistoryRecoveryEffectV1, String> {
        self.check_v1()?;
        action.check_wire_v1()?;
        if self.default.is_some() || now < self.opened_daa || now >= self.collect_until_daa {
            return Err("history recovery is terminal or outside its original paid window".into());
        }
        if let Some(answerer) = action.answerer_v1() {
            if !self.custodians.contains(&answerer) && answerer != self.initial.owner {
                return Err("history answerer has no literal signed custody grant".into());
            }
        }
        match action {
            PalwHistoryRecoveryActionV1::Demand => {
                if self.demand.is_some() {
                    return Err("history cursor already has an outstanding demand".into());
                }
                let request = self.remaining_request_v1().ok_or("history recovery already reached genesis")?;
                let until = now.checked_add(self.response_span_daa).ok_or("history response clock overflow")?;
                if until > self.recovery_until_daa {
                    return Err("new demand cannot extend its original recovery window".into());
                }
                self.demand =
                    Some(PalwHistoryRecoveryDemandV1 { request, opened_daa: now, answer_until_daa: until, buffers: BTreeMap::new() });
                Ok(PalwHistoryRecoveryEffectV1::Advanced)
            }
            PalwHistoryRecoveryActionV1::Begin { answerer, total_bytes } => {
                let demand = self.live_demand_v1(now)?;
                if demand.buffers.contains_key(answerer) {
                    return Err("history custodian cannot reset its bounded answer buffer".into());
                }
                demand.buffers.insert(*answerer, PalwHistoryRecoveryBufferV1 { total_bytes: *total_bytes, bytes: Vec::new() });
                Ok(PalwHistoryRecoveryEffectV1::Advanced)
            }
            PalwHistoryRecoveryActionV1::Part { answerer, offset, bytes } => {
                let demand = self.live_demand_v1(now)?;
                let buffer = demand.buffers.get_mut(answerer).ok_or("history answer has no accepted Begin")?;
                if *offset as usize != buffer.bytes.len()
                    || buffer.bytes.len().checked_add(bytes.len()).is_none_or(|end| end > buffer.total_bytes as usize)
                {
                    return Err("history part changes the original offset or bounded record length".into());
                }
                buffer.bytes.extend_from_slice(bytes);
                Ok(PalwHistoryRecoveryEffectV1::Advanced)
            }
            PalwHistoryRecoveryActionV1::Finish { answerer } => {
                let demand = self.live_demand_v1(now)?;
                let buffer = demand.buffers.get(answerer).ok_or("history answer has no accepted Begin")?;
                if buffer.bytes.len() != buffer.total_bytes as usize {
                    return Err("history record is not complete at EOF".into());
                }
                let record: PalwPrefixHistoryRecordV1 = borsh::from_slice(&buffer.bytes).map_err(|e| e.to_string())?;
                record.authenticates_request_v1(&demand.request)?;
                self.remaining_sequence = record.sequence - 1;
                self.remaining_head = record.previous_head;
                self.demand = None;
                Ok(PalwHistoryRecoveryEffectV1::Recovered(record))
            }
            PalwHistoryRecoveryActionV1::Collect => {
                let demand = self.demand.as_ref().ok_or("history default has no accepted demand")?;
                if now < demand.answer_until_daa {
                    return Err("history default cannot precede the original response deadline".into());
                }
                let default = PalwHistoryRecoveryDefaultV1 {
                    request: demand.request.clone(),
                    opened_daa: demand.opened_daa,
                    answer_until_daa: demand.answer_until_daa,
                    collected_daa: now,
                    custodians: self.custodians.clone(),
                };
                self.demand = None;
                self.default = Some(default.clone());
                Ok(PalwHistoryRecoveryEffectV1::Default(default))
            }
        }
    }
    fn live_demand_v1(&mut self, now: u64) -> Result<&mut PalwHistoryRecoveryDemandV1, String> {
        let demand = self.demand.as_mut().ok_or("history answer has no accepted demand")?;
        if now < demand.opened_daa || now >= demand.answer_until_daa {
            return Err("history answer is outside the demand's original response window".into());
        }
        Ok(demand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BlockHash,
        palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
        palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
        palw_prefix_family_court_v1::PalwPrefixCourtActionV1,
        tx::{TransactionId, TransactionOutpoint},
    };
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
    }
    fn record(previous: Hash64, sequence: u64) -> PalwPrefixHistoryRecordV1 {
        PalwPrefixHistoryRecordV1::new(
            previous,
            sequence,
            BlockHash::from_u64_word(sequence),
            100,
            500,
            PalwConstraintStreamMoveV1 {
                version: 1,
                claim: Hash64::from_u64_word(1),
                reserver: bond(7),
                reserved_daa: 90,
                seed_id: Hash64::from_u64_word(2),
                phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                action: PalwConstraintStreamActionV1::Prefix(PalwPrefixCourtActionV1::FinishTile),
                signature: vec![8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
        )
        .unwrap()
    }
    fn recovery(record: &PalwPrefixHistoryRecordV1) -> PalwHistoryRecoveryV1 {
        let request = PalwPrefixHistoryRequestV1 {
            claim: record.movement.claim,
            owner: record.movement.reserver,
            reserved_daa: record.movement.reserved_daa,
            seed_id: record.movement.seed_id,
            head: record.head_v1().unwrap(),
            sequence: record.sequence,
            material_until_daa: record.material_until_daa,
        };
        // More than six custodians proves the engine uses the native admitted
        // envelope, not a hard-coded five-seat network assumption.
        PalwHistoryRecoveryV1::new_v1(request, 110, 400, 450, 20, 10, (20..30).map(bond).collect()).unwrap()
    }
    fn answer(row: &mut PalwHistoryRecoveryV1, record: &PalwPrefixHistoryRecordV1, signer: PalwBondKeyV2, now: u64) {
        let bytes = borsh::to_vec(record).unwrap();
        row.apply_v1(&PalwHistoryRecoveryActionV1::Begin { answerer: signer, total_bytes: bytes.len() as u32 }, now).unwrap();
        for (part, chunk) in bytes.chunks(PALW_HISTORY_CUSTODY_PART_BYTES_V1).enumerate() {
            row.apply_v1(
                &PalwHistoryRecoveryActionV1::Part {
                    answerer: signer,
                    offset: (part * PALW_HISTORY_CUSTODY_PART_BYTES_V1) as u32,
                    bytes: chunk.to_vec(),
                },
                now,
            )
            .unwrap();
        }
        assert_eq!(
            row.apply_v1(&PalwHistoryRecoveryActionV1::Finish { answerer: signer }, now).unwrap(),
            PalwHistoryRecoveryEffectV1::Recovered(record.clone())
        );
    }
    #[test]
    fn rooted_backwards_recovery_survives_serialization_and_does_not_log_itself() {
        let first = record(Hash64::default(), 1);
        let second = record(first.head_v1().unwrap(), 2);
        let mut row = recovery(&second);
        row.apply_v1(&PalwHistoryRecoveryActionV1::Demand, 110).unwrap();
        answer(&mut row, &second, bond(20), 111);
        assert_eq!(row.remaining_request_v1().unwrap().head, first.head_v1().unwrap());
        let mut row: PalwHistoryRecoveryV1 = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
        row.check_v1().unwrap();
        row.apply_v1(&PalwHistoryRecoveryActionV1::Demand, 112).unwrap();
        // Owner answers from public material without becoming a liable custodian.
        answer(&mut row, &first, bond(7), 113);
        assert!(row.complete_v1());
        assert_eq!(row.initial.sequence, 2);
        assert!(row.apply_v1(&PalwHistoryRecoveryActionV1::Demand, 114).is_err());
    }
    #[test]
    fn hostile_slot_cannot_poison_honest_answer_or_reset_original_deadline() {
        let first = record(Hash64::default(), 1);
        let mut row = recovery(&first);
        row.apply_v1(&PalwHistoryRecoveryActionV1::Demand, 110).unwrap();
        row.apply_v1(&PalwHistoryRecoveryActionV1::Begin { answerer: bond(20), total_bytes: 1 }, 111).unwrap();
        row.apply_v1(&PalwHistoryRecoveryActionV1::Part { answerer: bond(20), offset: 0, bytes: vec![0] }, 111).unwrap();
        let before = row.clone();
        assert!(row.apply_v1(&PalwHistoryRecoveryActionV1::Finish { answerer: bond(20) }, 112).is_err());
        assert!(row.apply_v1(&PalwHistoryRecoveryActionV1::Begin { answerer: bond(20), total_bytes: 10 }, 112).is_err());
        assert!(row.apply_v1(&PalwHistoryRecoveryActionV1::Demand, 112).is_err());
        assert!(row.apply_v1(&PalwHistoryRecoveryActionV1::Begin { answerer: bond(90), total_bytes: 10 }, 112).is_err());
        assert_eq!(row, before);
        answer(&mut row, &first, bond(21), 113);
        assert!(row.complete_v1());
    }
    #[test]
    fn nonresponse_default_is_exact_once_and_never_compute_fraud() {
        let first = record(Hash64::default(), 1);
        let mut row = recovery(&first);
        row.apply_v1(&PalwHistoryRecoveryActionV1::Demand, 110).unwrap();
        let before = row.clone();
        assert!(row.apply_v1(&PalwHistoryRecoveryActionV1::Collect, 129).is_err());
        assert_eq!(row, before);
        assert!(row.apply_v1(&PalwHistoryRecoveryActionV1::Begin { answerer: bond(20), total_bytes: 10 }, 130).is_err());
        let PalwHistoryRecoveryEffectV1::Default(default) = row.apply_v1(&PalwHistoryRecoveryActionV1::Collect, 130).unwrap() else {
            panic!("nonresponse must be a DA default, not a semantic certificate");
        };
        assert_eq!(default.request, row.initial);
        assert_eq!(default.custodians, row.custodians);
        assert_eq!(default.answer_until_daa, 130);
        assert!(!default.custodians.contains(&row.initial.owner));
        assert!(row.apply_v1(&PalwHistoryRecoveryActionV1::Collect, 131).is_err());
        row.check_v1().unwrap();
        let mut altered = before;
        altered.demand.as_mut().unwrap().answer_until_daa += 1;
        assert!(altered.check_v1().is_err());
    }
    #[test]
    fn acquisition_quote_prices_every_slot_journal_and_checked_overflow() {
        let prices = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1().prices;
        let plan = history_recovery_resource_plan_v1(10, 10, prices).unwrap();
        assert!(plan.maximum_stream_bytes >= 10 * 11 * PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 as u64);
        assert!(plan.row_bytes > plan.maximum_stream_bytes);
        assert!(plan.inclusion_daa >= plan.maximum_moves);
        assert!(history_recovery_resource_plan_v1(u64::MAX, usize::MAX, prices).is_err());
        assert!(history_recovery_resource_plan_v1(10, 0, prices).is_err());
    }
    fn pledge() -> PalwHistoryCustodyPledgeV1 {
        PalwHistoryCustodyPledgeV1 {
            version: 1,
            claim: Hash64::from_u64_word(1),
            producer: bond(3),
            execution_root: Hash64::from_u64_word(4),
            job_identity: Hash64::from_u64_word(5),
            cu_ruleset_id: Hash64::from_u64_word(6),
            resource_plan_id: Hash64::from_u64_word(7),
            maximum_records: 100,
            maximum_custodians: 10,
            accept_until_daa: 110,
            material_until_daa: 500,
            signature: vec![8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }
    #[test]
    fn producer_pledge_identity_commits_all_terms_and_never_uses_signature_as_identity() {
        let pledge = pledge();
        let id = pledge.pledge_id_v1();
        for field in 0..11 {
            let mut changed = pledge.clone();
            match field {
                0 => changed.version += 1,
                1 => changed.claim = Hash64::from_u64_word(91),
                2 => changed.producer = bond(92),
                3 => changed.execution_root = Hash64::from_u64_word(93),
                4 => changed.job_identity = Hash64::from_u64_word(94),
                5 => changed.cu_ruleset_id = Hash64::from_u64_word(95),
                6 => changed.resource_plan_id = Hash64::from_u64_word(96),
                7 => changed.maximum_records += 1,
                8 => changed.maximum_custodians += 1,
                9 => changed.accept_until_daa += 1,
                _ => changed.material_until_daa += 1,
            }
            assert_ne!(changed.pledge_id_v1(), id);
        }
        let mut resigned = pledge.clone();
        resigned.signature[0] ^= 1;
        assert_eq!(resigned.pledge_id_v1(), id);
        let key = vec![0; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
        assert!(pledge.validate_signature_v1(&key, |_, _, _, _| false).is_err());
        assert!(pledge.validate_signature_v1(&[], |_, _, _, _| true).is_err());
        // A synthetic verifier only tests message/domain dispatch, not ML-DSA.
        pledge
            .validate_signature_v1(&key, |pk, message, signature, domain| {
                pk == key && message == id.as_byte_slice() && signature == pledge.signature && domain == PALW_HISTORY_PLEDGE_CONTEXT_V1
            })
            .unwrap();
    }
    #[test]
    fn valid_grant_requires_its_own_signature_and_exact_literal_receipt() {
        let pledge = pledge();
        let receipt = crate::palw_panel_v2::PalwSeatReceiptV2 {
            claim: pledge.claim,
            seat_bond: bond(20),
            signed_daa: 101,
            verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            signature: vec![9; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let grant = PalwHistoryCustodyGrantV1 {
            version: 1,
            claim: receipt.claim,
            seat: receipt.seat_bond,
            signed_daa: receipt.signed_daa,
            pledge_id: pledge.pledge_id_v1(),
            valid_receipt_digest: PalwHistoryCustodyGrantV1::receipt_digest_v1(&receipt),
            signature: vec![10; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        grant.check_receipt_v1(&receipt).unwrap();
        let key = vec![0; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
        assert!(grant.validate_signature_v1(&receipt, &key, |_, _, _, _| false).is_err());
        assert!(grant.validate_signature_v1(&receipt, &[], |_, _, _, _| true).is_err());
        let mut changed = receipt.clone();
        changed.signature[0] ^= 1;
        assert!(grant.check_receipt_v1(&changed).is_err());
        changed = receipt.clone();
        changed.verdict = crate::palw_panel_v2::PalwReceiptVerdictV2::Sampled;
        assert!(grant.check_receipt_v1(&changed).is_err());
        let mut changed = grant.clone();
        changed.pledge_id = Hash64::from_u64_word(99);
        assert_ne!(changed.signing_message_v1(), grant.signing_message_v1());
        assert_ne!(PALW_HISTORY_GRANT_CONTEXT_V1, PALW_HISTORY_PLEDGE_CONTEXT_V1);
    }
    #[test]
    fn explicit_fp_wrapper_is_bounded_nonrecursive_and_rejects_ignored_tails() {
        let envelope = PalwHistoryFpEnvelopeV1 { version: 1, original_payload: vec![3, 0, 8], pledge: pledge() };
        let encoded = envelope.encode_v1().unwrap();
        assert_eq!(PalwHistoryFpEnvelopeV1::decode_v1(&encoded).unwrap(), envelope);
        assert!(envelope.original_fp_v1().is_err());
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(PalwHistoryFpEnvelopeV1::decode_v1(&trailing).is_err());
        let mut recursive = envelope.clone();
        recursive.original_payload = encoded;
        assert!(recursive.encode_v1().is_err());
        let mut large = envelope;
        large.original_payload = vec![0; crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES + 1];
        assert!(large.encode_v1().is_err());
        assert!(PalwHistoryFpEnvelopeV1::decode_v1(&vec![0; PALW_HISTORY_FP_ENVELOPE_MAX_BYTES_V1 + 1]).is_err());
    }
}
