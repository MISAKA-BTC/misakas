//! Explicit typed-stage source/class namespace. The native stage14 fence is
//! independent of funded text11 and no preset approves its reward route.
use crate::{
    Hash64, config::params::ForkActivation, palw_flat_execution_v11::NativeExecutionPoliciesV11,
    palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_inputs12::source14::{FlatStageClassWireV14, FlatStageSourceWireV14},
    hash::{Digest, object_id},
};
use std::io::Cursor;

pub const STAGE_TABLE_V14: u8 = 112;
pub const STAGE_ACTION_BIND_V14: u8 = 0;
pub const STAGE_ACTION_REGISTER_V14: u8 = 1;
pub const STAGE_ACTION_FUND_V14: u8 = 2;
pub const STAGE_ACTION_COMMIT_V14: u8 = 3;
pub const STAGE_ACTION_CLOSE_V14: u8 = 4;
pub const STAGE_ACTION_SOURCE_SEAL_V14: u8 = 5;
pub const STAGE_ACTION_SOURCE_REVEAL_V14: u8 = 6;
pub const STAGE_ACTION_ACCEPT_V14: u8 = 13;
pub const STAGE_ACTION_ACCEPT_V18: u8 = 14;
pub const STAGE_ACTION_COLLECT_V14: u8 = 15;
pub const STAGE_CATALOG_MAX_V14: usize = 8;
pub const STAGE_NATIVE_INPUT_BYTES_MAX_V14: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStagePoliciesV14 {
    pub version: u16,
    pub execution: NativeExecutionPoliciesV11,
    pub source_reservation: u64,
    pub source_maturity: u64,
    pub source_liability: u64,
    pub catalog_cap: u8,
}
impl NativeStagePoliciesV14 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/stage-policies/v14", self)
    }
    pub fn check(&self, network_domain: Digest) -> Result<(), String> {
        self.execution.check(network_domain)?;
        if self.version != 14
            || self.source_reservation == 0
            || self.source_maturity == 0
            || self.source_liability <= self.source_maturity
            || self.catalog_cap == 0
            || self.catalog_cap as usize > STAGE_CATALOG_MAX_V14
        {
            return Err("typed stage policy lacks bounded source liability/catalog semantics".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStageExecutionFenceV14 {
    pub activation: ForkActivation,
    pub policies: NativeStagePoliciesV14,
}

pub fn stage_key_v14<T: BorshSerialize>(kind: u8, value: &T) -> Vec<u8> {
    borsh::to_vec(&(kind, value)).expect("bounded native stage key")
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageSourceHeaderV14 {
    pub version: u16,
    pub id: Digest,
    pub program_root: Digest,
    pub input_declarations_root: Digest,
    pub parameters_root: Digest,
    pub inventory_root: Digest,
    pub relation_root: Digest,
    pub source_bytes_len: u32,
    pub program_bytes_len: u32,
    pub pc_instances: u32,
    pub max_proof_bytes: u32,
    pub binder: PalwBondKeyV2,
    pub bound_daa: u64,
    pub matures_daa: u64,
    pub final_daa: u64,
    pub reserved: u64,
    pub refuted: bool,
}
impl NativeStageSourceHeaderV14 {
    pub fn reserved_at(&self, daa: u64) -> u64 {
        if self.refuted || daa >= self.final_daa { 0 } else { self.reserved }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeStageSourceRowV14 {
    pub header: NativeStageSourceHeaderV14,
    pub source_bytes: Vec<u8>,
}
impl NativeStageSourceRowV14 {
    pub fn checked_source(&self) -> Result<FlatStageSourceWireV14, String> {
        if self.header.version != 14 || self.header.source_bytes_len as usize != self.source_bytes.len() {
            return Err("stored stage source fixed header differs".into());
        }
        let source = FlatStageSourceWireV14::from_bytes(&self.source_bytes)?;
        let checked = source.check()?;
        if checked.id() != self.header.id
            || checked.program_root() != self.header.program_root
            || checked.input_declarations_root() != self.header.input_declarations_root
            || checked.parameters().root() != self.header.parameters_root
            || checked.inventory_root() != self.header.inventory_root
            || checked.relation_root() != self.header.relation_root
            || source.program_bytes.len() != self.header.program_bytes_len as usize
            || source.source_parameters.by_instance.len() != self.header.pc_instances as usize
        {
            return Err("stored stage source differs from independent typed namespace".into());
        }
        Ok(source)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageClassHeaderV14 {
    pub version: u16,
    pub id: Digest,
    pub source_binding: Digest,
    pub template_root: Digest,
    pub policy_root: Digest,
    pub max_scope_root: Digest,
    pub max_resource_root: Digest,
    pub wire_bytes_len: u32,
    pub owner: PalwBondKeyV2,
    pub admitted_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeStageClassRowV14 {
    pub header: NativeStageClassHeaderV14,
    pub class_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct StageFundRequestV14 {
    pub version: u16,
    pub class: Digest,
    pub producer: PalwBondKeyV2,
    pub reward: u64,
    pub acceptance: Digest,
    pub job_bytes: Vec<u8>,
    pub input_images: Vec<Vec<u8>>,
}
impl StageFundRequestV14 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        use std::io::Read;
        if raw.len() > STAGE_NATIVE_INPUT_BYTES_MAX_V14 + 2112 {
            return Err("stage funded input carrier cap".into());
        }
        let mut r = Cursor::new(raw);
        let version = u16::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        if version != 14 {
            return Err("unknown funded stage version".into());
        }
        let class = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let producer = PalwBondKeyV2::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let reward = u64::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let acceptance = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
        if n > 2 + 4 * 64 + 16 + 4 + 16 * 64 || n > raw.len().saturating_sub(r.position() as usize) {
            return Err("funded stage job count exceeds wire cap".into());
        }
        let mut job_bytes = vec![0; n];
        r.read_exact(&mut job_bytes).map_err(|e| e.to_string())?;
        misaka_palw_kernel::flat_inputs12::bounded14::FlatStageJobV14::from_bytes(&job_bytes)?;
        let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
        if n == 0 || n > misaka_palw_kernel::flat_inputs12::INPUT_COUNT_MAX_V12 {
            return Err("funded stage image count cap".into());
        }
        let mut input_images = Vec::with_capacity(n);
        let mut total = 0usize;
        for _ in 0..n {
            let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
            total = total.checked_add(n).ok_or("funded stage input sum overflow")?;
            if total > STAGE_NATIVE_INPUT_BYTES_MAX_V14 || n > raw.len().saturating_sub(r.position() as usize) {
                return Err("funded stage image exceeds available bounded bytes".into());
            }
            let mut image = vec![0; n];
            r.read_exact(&mut image).map_err(|e| e.to_string())?;
            input_images.push(image);
        }
        let out = Self { version, class, producer, reward, acceptance, job_bytes, input_images };
        if r.position() as usize != raw.len() || borsh::to_vec(&out).map_err(|e| e.to_string())? != raw {
            return Err("funded stage carrier has surplus or noncanonical bytes".into());
        }
        Ok(out)
    }
}

/// Signed by the original producer, binding exact independently derived
/// preparation before a requester can occupy any producer quota or job slot.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct StageAcceptRequestV14 {
    pub version: u16,
    pub class: Digest,
    pub requester: PalwBondKeyV2,
    pub reward: u64,
    pub expires_daa: u64,
    pub source: Digest,
    pub policy: Digest,
    pub admission: Digest,
    pub quota: Digest,
    pub job_bytes: Vec<u8>,
}
impl StageAcceptRequestV14 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        use std::io::Read;
        if raw.len() > 2048 {
            return Err("stage producer acceptance carrier exceeds cap".into());
        }
        let mut r = Cursor::new(raw);
        let version = u16::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        if version != 14 {
            return Err("unknown stage acceptance version".into());
        }
        let class = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let requester = PalwBondKeyV2::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let reward = u64::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let expires_daa = u64::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let source = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let policy = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let admission = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let quota = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
        if n > 2 + 4 * 64 + 16 + 4 + 16 * 64 || n != raw.len().saturating_sub(r.position() as usize) {
            return Err("stage acceptance job count differs from bounded exact wire".into());
        }
        let mut job_bytes = vec![0; n];
        r.read_exact(&mut job_bytes).map_err(|e| e.to_string())?;
        misaka_palw_kernel::flat_inputs12::bounded14::FlatStageJobV14::from_bytes(&job_bytes)?;
        Ok(Self { version, class, requester, reward, expires_daa, source, policy, admission, quota, job_bytes })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageAcceptanceV14 {
    pub version: u16,
    pub id: Digest,
    pub class: Digest,
    pub source: Digest,
    pub policy: Digest,
    pub job: Digest,
    pub admission: Digest,
    pub quota: Digest,
    pub requester: PalwBondKeyV2,
    pub producer: PalwBondKeyV2,
    pub requester_operator: Digest,
    pub producer_operator: Digest,
    pub nonce: Digest,
    pub reward: u64,
    pub accepted_daa: u64,
    pub expires_daa: u64,
}
impl NativeStageAcceptanceV14 {
    pub fn content_id(&self) -> Digest {
        object_id(
            b"misaka-palw/native/stage-producer-acceptance/v14",
            &(
                self.version,
                self.class,
                self.source,
                self.policy,
                self.job,
                self.admission,
                self.quota,
                self.requester,
                self.producer,
                self.requester_operator,
                self.producer_operator,
                self.nonce,
                self.reward,
                self.accepted_daa,
                self.expires_daa,
            ),
        )
    }
}
/// New consent grammar explicitly names the additional producer-owned source
/// and trace delivery contracts. Old action13 metadata never authorizes them.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct StageAcceptRequestV18 {
    pub version: u16,
    pub preparation_bytes: Vec<u8>,
    pub source_plan: Digest,
    pub trace_plan: Digest,
    pub availability_terms: Digest,
    pub fee_plan: Digest,
}
impl StageAcceptRequestV18 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() < 262 || raw.len() > 2310 || u16::from_le_bytes(raw[..2].try_into().unwrap()) != 18 {
            return Err("stage consent18 version/carrier extent".into());
        }
        let n = u32::from_le_bytes(raw[2..6].try_into().unwrap()) as usize;
        if n > 2048 || n != raw.len() - 262 {
            return Err("stage consent18 preparation count exceeds exact bounded frame".into());
        }
        StageAcceptRequestV14::from_bytes(&raw[6..6 + n])?;
        Ok(Self {
            version: 18,
            preparation_bytes: raw[6..6 + n].to_vec(),
            source_plan: raw[6 + n..70 + n].try_into().unwrap(),
            trace_plan: raw[70 + n..134 + n].try_into().unwrap(),
            availability_terms: raw[134 + n..198 + n].try_into().unwrap(),
            fee_plan: raw[198 + n..262 + n].try_into().unwrap(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageAcceptanceV18 {
    pub version: u16,
    pub id: Digest,
    pub preparation: NativeStageAcceptanceV14,
    pub source_plan: Digest,
    pub trace_plan: Digest,
    pub availability_terms: Digest,
    pub fee_plan: Digest,
}
impl NativeStageAcceptanceV18 {
    pub fn content_id(&self) -> Digest {
        object_id(
            b"misaka-palw/native/stage-source-trace-consent/v18",
            &(self.version, self.preparation.content_id(), self.source_plan, self.trace_plan, self.availability_terms, self.fee_plan),
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageJobHeaderV14 {
    pub version: u16,
    pub id: Digest,
    pub class: Digest,
    pub acceptance: Digest,
    pub source_binding: Digest,
    pub policy_root: Digest,
    pub admission_root: Digest,
    pub stage_quota_root: Digest,
    pub quota_record: misaka_palw_kernel::flat_quota11::QuotaPlanRecordV11,
    pub original_book_root: Digest,
    pub book_root: Digest,
    pub book_bytes_len: u32,
    pub requester: PalwBondKeyV2,
    pub producer: PalwBondKeyV2,
    pub requester_operator: Digest,
    pub producer_operator: Digest,
    pub requester_payout: Hash64,
    pub producer_payout: Hash64,
    pub positions: u32,
    pub input_raw_bytes: u64,
    pub input_count: u16,
    pub deadline_daa: u64,
    pub producer_reserved: u64,
    pub terminal: misaka_palw_kernel::flat_quota11::stage14::StageTerminalV14,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeStageJobRowV14 {
    pub header: NativeStageJobHeaderV14,
    pub book_bytes: Vec<u8>,
    pub input_images: Vec<Vec<u8>>,
    pub trace_bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStagePrincipalRefundV14 {
    pub version: u16,
    pub job: Digest,
    pub original_book: Digest,
    pub terminal_book: Digest,
    pub acceptance18: Digest,
    pub requester: PalwBondKeyV2,
    pub payout: Hash64,
    pub queue: Hash64,
    pub amount: u64,
    pub established_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageNegativeSourceFactV14 {
    pub version: u16,
    pub relation: Digest,
    pub source: Digest,
    pub source_program: Digest,
    pub parameters: Digest,
    pub inventory: Digest,
    pub proof: Digest,
    pub established_daa: u64,
    pub reporter: PalwBondKeyV2,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageFeePlanV20 {
    pub version: u16,
    pub class: Digest,
    pub quota_plan: Digest,
    pub original_preparation: Digest,
    pub governing_policy: Digest,
    pub reward: u64,
    pub maximum: u64,
    pub trace_units: u32,
    pub source_units: u32,
    pub trace_unit_fee: u64,
    pub source_unit_fee: u64,
}
impl NativeStageFeePlanV20 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/stage-original-delivery-fee-plan/v20", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageFeesV20 {
    pub version: u16,
    pub job: Digest,
    pub original_book: Digest,
    pub consent18: Digest,
    pub plan: NativeStageFeePlanV20,
    pub earned: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStagePursuitV14 {
    pub version: u16,
    pub job: Digest,
    pub class: Digest,
    pub original_book: Digest,
    pub consent18: Digest,
    pub open_demands: u32,
    pub retained_seals: u32,
    pub retained_discovery: u32,
    pub requests_admitted: u32,
    pub required_until: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageCollectedV14 {
    pub version: u16,
    pub job: Digest,
    pub class: Digest,
    pub original_book: Digest,
    pub terminal_book: Digest,
    pub consent18: Digest,
    pub requester_operator: Digest,
    pub original_nonce: Digest,
    pub principal_credit: Digest,
    pub source_retired: Digest,
    pub trace_retired: Digest,
    pub collected_daa: u64,
}
pub fn stage_delivery_fee_plan_v20(
    ledger: &misaka_palw_kernel::ledger::LedgerPolicyV1,
    p: &NativeStagePoliciesV14,
    request: &StageAcceptRequestV14,
    source_units: u32,
) -> Result<NativeStageFeePlanV20, String> {
    p.check(ledger.network_domain)?;
    if request.version != 14
        || request.class == [0; 64]
        || request.policy != p.root()
        || request.reward == 0
        || request.reward > p.execution.execution.reward_cap
        || source_units == 0
        || source_units > 1 << 20
    {
        return Err("stage fee quote changes original principals/preparation/policy/reward".into());
    }
    let trace_unit_fee =
        ledger.demand_bond.max(ledger.dismissal_fee_v1(crate::palw_model_artifact_v2::model_artifact_work_v2(0, 16 << 10).unwrap()));
    let source_unit_fee = ledger.demand_bond.max(
        ledger.dismissal_fee_v1(
            crate::palw_model_artifact_v2::model_artifact_work_v2(
                0,
                misaka_palw_kernel::flat_quota11::source16::SOURCE_ANSWER_MAX_BYTES_V16 as usize,
            )
            .unwrap(),
        ),
    );
    let possible =
        (trace_unit_fee as u128) * (p.execution.quota.future_attempts[2] as u128) + (source_unit_fee as u128) * (source_units as u128);
    let maximum = (p.execution.execution.reward_cap - request.reward).min(possible.min(u64::MAX as u128) as u64);
    if request.reward as u128
        + maximum as u128
        + p.execution.execution.external_gain_cap as u128
        + p.execution.execution.default_obligation as u128
        > p.execution.execution.required_producer_reserve as u128
    {
        return Err("stage original reserve fails reward/all capped delivery fees/external/default gain".into());
    }
    Ok(NativeStageFeePlanV20 {
        version: 20,
        class: request.class,
        quota_plan: request.quota,
        original_preparation: object_id(b"misaka-palw/native/stage-preparation/v14", request),
        governing_policy: object_id(b"misaka-palw/native/stage-governing-ledger-policy/v20", ledger),
        reward: request.reward,
        maximum,
        trace_units: p.execution.quota.future_attempts[2],
        source_units,
        trace_unit_fee,
        source_unit_fee,
    })
}
pub fn stage_availability_terms_v18(
    p: &NativeStagePoliciesV14,
    request: &StageAcceptRequestV14,
    source: Digest,
    trace: Digest,
    fee: Digest,
) -> Digest {
    object_id(
        b"misaka-palw/native/stage-original-availability-terms/v18",
        &(
            18u16,
            p.root(),
            object_id(b"misaka-palw/native/stage-preparation/v14", request),
            source,
            trace,
            fee,
            &p.execution.execution.horizons,
            request.expires_daa / p.execution.quota.interval_daa,
            misaka_palw_kernel::flat_quota11::source16::SOURCE_ANSWER_MAX_BYTES_V16,
            STAGE_NATIVE_INPUT_BYTES_MAX_V14 as u64,
        ),
    )
}

/// Bounded source R/PC court over the original V2 real weight namespace.
/// Callers must authenticate the original row and prepay its court before
/// invoking this helper; its result grants no native payout or job privilege.
pub fn verify_stage_source_proof_v14(row: &NativeStageSourceRowV14, proof_bytes: &[u8]) -> Result<(), String> {
    let source = row.checked_source()?;
    let source = source.check()?;
    let bound = misaka_palw_kernel::flat_execution11::native_source_court_bound_v11(
        row.header.program_bytes_len as usize,
        source.parameters(),
    )?;
    if bound.max_proof_bytes != row.header.max_proof_bytes as u64 {
        return Err("original stage source proof cap differs from independent derivation".into());
    }
    let proof = crate::palw_model_artifact_v2::decode_model_artifact_proof_v2(
        proof_bytes,
        row.header.pc_instances,
        row.header.max_proof_bytes,
    )
    .map_err(|e| e.to_string())?;
    let view = source.program().v1_view();
    let inventory = crate::palw_tir_artifact_v1::PalwTirModelInventoryV2::new_stage14(&view, source.program())?;
    crate::palw_onboarding_v1::verify_artifact_mismatch_v2(
        &inventory,
        crate::Hash64::from_bytes(source.inventory_root()),
        crate::Hash64::from_bytes(source.parameters().root()),
        &proof,
    )
    .map_err(str::to_owned)
}

impl PalwKernelRouteStateV1 {
    pub fn stage_source_refuted_relation_v14(&self, relation: &Digest) -> bool {
        self.aux_row::<NativeStageNegativeSourceFactV14>(STAGE_TABLE_V14, &stage_key_v14(14, relation))
            .is_some_and(|f| f.version == 14 && f.relation == *relation && f.proof != [0; 64])
    }
    pub fn stage_acceptance_v18(&self, id: &Digest) -> Option<NativeStageAcceptanceV18> {
        let row: NativeStageAcceptanceV18 = self.aux_row(STAGE_TABLE_V14, &stage_key_v14(10, id))?;
        (row.version == 18
            && row.id == *id
            && row.content_id() == *id
            && row.preparation.version == 14
            && row.preparation.id == row.preparation.content_id()
            && row.source_plan != [0; 64]
            && row.trace_plan != [0; 64]
            && row.fee_plan != [0; 64]
            && row.availability_terms != [0; 64])
            .then_some(row)
    }
    pub fn stage_acceptance_v14(&self, id: &Digest) -> Option<NativeStageAcceptanceV14> {
        let row: NativeStageAcceptanceV14 = self.aux_row(STAGE_TABLE_V14, &stage_key_v14(9, id))?;
        (row.version == 14 && row.id == *id && row.content_id() == *id).then_some(row)
    }
    pub fn stage_job_header_v14(&self, id: &Digest) -> Option<NativeStageJobHeaderV14> {
        let raw = self.aux.get(&(STAGE_TABLE_V14, stage_key_v14(5, id)))?;
        let h = NativeStageJobHeaderV14::deserialize_reader(&mut Cursor::new(raw)).ok()?;
        (h.version == 14 && h.id == *id).then_some(h)
    }
    pub fn stage_job_row_v14(&self, id: &Digest) -> Option<NativeStageJobRowV14> {
        use std::io::Read;
        let raw = self.aux.get(&(STAGE_TABLE_V14, stage_key_v14(5, id)))?;
        let mut r = Cursor::new(raw);
        let h = NativeStageJobHeaderV14::deserialize_reader(&mut r).ok()?;
        if h.version != 14
            || h.id != *id
            || h.book_bytes_len > 4096
            || h.input_raw_bytes > STAGE_NATIVE_INPUT_BYTES_MAX_V14 as u64
            || h.input_count > 16
        {
            return None;
        }
        fn vec(r: &mut Cursor<&Vec<u8>>, cap: usize) -> Option<Vec<u8>> {
            let n = u32::deserialize_reader(r).ok()? as usize;
            if n > cap || n > r.get_ref().len().checked_sub(r.position() as usize)? {
                return None;
            }
            let mut raw = std::vec![0; n];
            r.read_exact(&mut raw).ok()?;
            Some(raw)
        }
        let book_bytes = vec(&mut r, h.book_bytes_len as usize)?;
        if book_bytes.len() != h.book_bytes_len as usize {
            return None;
        }
        let n = u32::deserialize_reader(&mut r).ok()? as usize;
        if n != h.input_count as usize {
            return None;
        }
        let mut input_images = Vec::with_capacity(n);
        let mut total = 0usize;
        for _ in 0..n {
            let raw = vec(&mut r, STAGE_NATIVE_INPUT_BYTES_MAX_V14.checked_sub(total)?)?;
            total = total.checked_add(raw.len())?;
            input_images.push(raw);
        }
        if total != h.input_raw_bytes as usize {
            return None;
        }
        let trace_bytes = vec(&mut r, 2 << 20)?;
        if r.position() as usize != raw.len() {
            return None;
        }
        let book = misaka_palw_kernel::flat_quota11::stage14::StageJobBookV14::from_rooted_bytes(&book_bytes, h.book_root).ok()?;
        if book.id() != h.id
            || book.class_id() != h.class
            || book.acceptance() != h.acceptance
            || book.source_binding() != h.source_binding
            || book.native_policy_root() != h.policy_root
            || book.admission().root() != h.admission_root
            || book.original_funded_root() != h.original_book_root
            || book.requester_operator() != h.requester_operator
            || book.producer_operator() != h.producer_operator
            || book.deadline_daa() != h.deadline_daa
            || book.producer_reserved() != h.producer_reserved
            || book.terminal() != h.terminal
            || book.job().positions != h.positions
        {
            return None;
        }
        Some(NativeStageJobRowV14 { header: h, book_bytes, input_images, trace_bytes })
    }
    pub fn stage_source_header_v14(&self, id: &Digest) -> Option<NativeStageSourceHeaderV14> {
        let raw = self.aux.get(&(STAGE_TABLE_V14, stage_key_v14(0, id)))?;
        let header = NativeStageSourceHeaderV14::deserialize_reader(&mut Cursor::new(raw)).ok()?;
        (header.version == 14 && header.id == *id).then_some(header)
    }
    pub fn stage_source_row_v14(&self, id: &Digest) -> Option<NativeStageSourceRowV14> {
        let raw = self.aux.get(&(STAGE_TABLE_V14, stage_key_v14(0, id)))?;
        let mut r = Cursor::new(raw);
        let header = NativeStageSourceHeaderV14::deserialize_reader(&mut r).ok()?;
        let n = u32::deserialize_reader(&mut r).ok()? as usize;
        if header.version != 14
            || header.id != *id
            || n != header.source_bytes_len as usize
            || n > misaka_palw_kernel::flat_inputs12::source14::STAGE_SOURCE_MAX_BYTES_V14
            || n != raw.len().checked_sub(r.position() as usize)?
        {
            return None;
        }
        Some(NativeStageSourceRowV14 { header, source_bytes: raw[r.position() as usize..].to_vec() })
    }
    pub fn stage_class_header_v14(&self, id: &Digest) -> Option<NativeStageClassHeaderV14> {
        let raw = self.aux.get(&(STAGE_TABLE_V14, stage_key_v14(2, id)))?;
        let header = NativeStageClassHeaderV14::deserialize_reader(&mut Cursor::new(raw)).ok()?;
        (header.version == 14 && header.id == *id).then_some(header)
    }
    pub fn stage_class_wire_v14(&self, id: &Digest) -> Option<FlatStageClassWireV14> {
        let raw = self.aux.get(&(STAGE_TABLE_V14, stage_key_v14(2, id)))?;
        let mut r = Cursor::new(raw);
        let h = NativeStageClassHeaderV14::deserialize_reader(&mut r).ok()?;
        let n = u32::deserialize_reader(&mut r).ok()? as usize;
        if h.version != 14
            || h.id != *id
            || n != h.wire_bytes_len as usize
            || n > misaka_palw_kernel::flat_lengths7::FLAT_LENGTH_MAX_WIRE_BYTES_V7 + 138
            || n != raw.len().checked_sub(r.position() as usize)?
        {
            return None;
        }
        FlatStageClassWireV14::from_bytes(&raw[r.position() as usize..]).ok()
    }
    pub fn stage_owner_ids_v14(&self, kind: u8, owner: &PalwBondKeyV2) -> Option<Vec<Digest>> {
        let Some(raw) = self.aux.get(&(STAGE_TABLE_V14, stage_key_v14(kind, owner))) else {
            return Some(vec![]);
        };
        if raw.len() < 4 || raw.len() > 4 + STAGE_CATALOG_MAX_V14 * 64 {
            return None;
        }
        let n = u32::from_le_bytes(raw[..4].try_into().ok()?) as usize;
        if n > STAGE_CATALOG_MAX_V14 || raw.len() != 4 + n * 64 {
            return None;
        }
        borsh::from_slice(raw).ok()
    }
    pub fn stage_source_reserved_v14(&self, owner: &PalwBondKeyV2, daa: u64) -> u128 {
        self.stage_owner_ids_v14(1, owner)
            .unwrap_or_default()
            .iter()
            .filter_map(|id| self.stage_source_header_v14(id))
            .filter(|h| h.binder == *owner)
            .map(|h| h.reserved_at(daa) as u128)
            .sum()
    }
    pub fn stage_producer_reserved_v14(&self, owner: &PalwBondKeyV2) -> u128 {
        self.stage_owner_ids_v14(7, owner)
            .unwrap_or_default()
            .iter()
            .filter_map(|id| self.stage_job_header_v14(id))
            .filter(|h| h.producer == *owner)
            .map(|h| h.producer_reserved as u128)
            .sum()
    }
}
