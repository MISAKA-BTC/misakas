//! Owning39 native source/class authority. No Source28, Text29 or old quota
//! record is reinterpreted as39. Registration alone grants no Final or credit.
use crate::{config::params::ForkActivation, palw_flat_execution_v11::NativeExecutionPoliciesV11};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_resources39::{CheckedFlatResourcesV39, FlatCourtScopeV39, FlatResourceVectorV39, derive_flat_resources_v39},
    flat_source39::CheckedSourceV39,
    hash::{Digest, object_id},
};

pub const NATIVE_ACTION_FIRST_V39: u8 = 41;
pub const NATIVE_ACTION_LAST_V39: u8 = 79;
pub const REGISTER_SOURCE_ACTION_V39: u8 = 41;
pub const REGISTER_CLASS_ACTION_V39: u8 = 42;
pub const SOURCE_START_ACTION_V39: u8 = 57;
pub const SOURCE_PART_ACTION_V39: u8 = 58;
pub const SOURCE_ASSEMBLE_ACTION_V39: u8 = 59;
pub const SOURCE_COLLECT_ACTION_V39: u8 = 60;
pub const SOURCE_PART_BYTES_V39: usize = 32 << 10;
pub const SOURCE_UPLOAD_MAX_BYTES_V39: usize = misaka_palw_kernel::flat_source39::SOURCE39_MAX_BYTES;
pub const SOURCE_UPLOAD_MAX_PARTS_V39: usize = SOURCE_UPLOAD_MAX_BYTES_V39.div_ceil(SOURCE_PART_BYTES_V39);

/// Optional immutable data archive for bodies whose live retention was released.
/// Returned bytes carry no authority. Auditors authenticate them against the
/// original source/class headers in their current independently anchored state.
pub trait NativeOriginalSourceArchiveV39 {
    fn source(&self, id: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String>;
    fn class(&self, id: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String>;
    /// The exact stored consent row 27 of a job collected by action 53. The
    /// auditor authenticates it against the immutable collection record
    /// (consent root, byte length and stored-body digest); none is authority.
    fn consent(&self, job: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String> {
        let _ = (job, maximum);
        Ok(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSourceUploadStartV39 {
    pub version: u16,
    pub source: Digest,
    pub bytes: u32,
    pub count: u16,
    pub sequence: u64,
    pub deadline: u64,
    pub body: Digest,
}
impl NativeSourceUploadStartV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 152 {
            return Err("source upload39 fixed Start extent".into());
        }
        borsh::from_slice(raw).map_err(|_| "source upload39 exact Start grammar".into())
    }
    pub fn check(&self, now: u64, until: u64) -> Result<(), String> {
        if self.version != 39
            || self.source == [0; 64]
            || self.body == [0; 64]
            || self.bytes == 0
            || self.bytes as usize > SOURCE_UPLOAD_MAX_BYTES_V39
            || self.count as usize != (self.bytes as usize).div_ceil(SOURCE_PART_BYTES_V39)
            || self.count as usize > SOURCE_UPLOAD_MAX_PARTS_V39
            || self.deadline <= now
            || self.deadline > until
        {
            return Err("source upload39 original byte/part/clock reservation differs".into());
        }
        Ok(())
    }
    pub fn id(&self, owner: crate::palw_state_v2::PalwBondKeyV2) -> Digest {
        object_id(b"misaka-palw/native/source-upload-original/v39", &(owner, self))
    }
}
#[derive(Clone, Copy)]
pub struct BorrowedSourceUploadPartV39<'a> {
    pub id: Digest,
    pub index: u16,
    pub bytes: &'a [u8],
}
pub fn borrowed_source_upload_part_v39(raw: &[u8]) -> Option<BorrowedSourceUploadPartV39<'_>> {
    if raw.len() < 72 || raw.len() > 72 + SOURCE_PART_BYTES_V39 || raw[..2] != 39u16.to_le_bytes() {
        return None;
    }
    let n = u32::from_le_bytes(raw[68..72].try_into().ok()?) as usize;
    if n != raw.len() - 72 {
        return None;
    }
    Some(BorrowedSourceUploadPartV39 {
        id: raw[2..66].try_into().ok()?,
        index: u16::from_le_bytes(raw[66..68].try_into().ok()?),
        bytes: &raw[72..],
    })
}

pub fn source_protocol_root_v39() -> Digest {
    object_id(
        b"misaka-palw/native/owning-source-protocol/v39",
        &(
            39u16,
            41u8,
            42u8,
            57u8,
            58u8,
            59u8,
            60u8,
            NATIVE_ACTION_LAST_V39,
            crate::palw_native_master_v39::MASTER_REGISTER_MAX_BYTES_V39 as u64,
            64u16,
            8u8,
            SOURCE_PART_BYTES_V39 as u64,
            SOURCE_UPLOAD_MAX_BYTES_V39 as u64,
            misaka_palw_kernel::flat_resources39::ordinary_court_id_v39(),
            crate::palw_native_job_v39::final_conditions_id_v39(),
            b"canonical39;real-PC-only;inputs-separate;original-context;paid-before-parse;owning-material61-64;metadata67-70;master-body-pages93x32768;retention71-72;immutable-headers-exact-rehydration;class-holds94-95;live-payers96-97;source-active-classes98;mining73-79".to_vec(),
        ),
    )
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionPoliciesV39 {
    pub version: u16,
    pub network_domain: Digest,
    pub execution: NativeExecutionPoliciesV11,
    pub final_policy: Digest,
    pub physical_service: Digest,
    pub source_protocol39: Digest,
    pub maximum_horizon: u64,
    pub catalog_cap: u8,
}
impl NativeExecutionPoliciesV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/owning-execution-policy/v39", self)
    }
    pub fn check(&self, network_domain: Digest) -> Result<(), String> {
        self.execution.check(network_domain)?;
        if self.version != 39
            || self.network_domain != network_domain
            || self.final_policy == [0; 64]
            || self.physical_service == [0; 64]
            || self.source_protocol39 != source_protocol_root_v39()
            || self.maximum_horizon == 0
            || self.maximum_horizon >= self.execution.quota.interval_daa
            || self.catalog_cap == 0
            || self.catalog_cap > 64
        {
            return Err(
                "native39 requires original network/shared quota/Final/physical/source authority and bounded catalog/horizon".into()
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwNativeExecutionFenceV39 {
    pub activation: ForkActivation,
    pub policies: NativeExecutionPoliciesV39,
}
impl PalwNativeExecutionFenceV39 {
    pub fn unapproved_probe_at(activation: ForkActivation, network_domain: Digest) -> Self {
        let old = crate::palw_funded_execution_v11::PalwFundedExecutionFenceV11::unapproved_probe_at(activation, network_domain);
        Self {
            activation,
            policies: NativeExecutionPoliciesV39 {
                version: 39,
                network_domain,
                execution: old.policies,
                final_policy: [0; 64],
                physical_service: [0; 64],
                source_protocol39: source_protocol_root_v39(),
                maximum_horizon: 900,
                catalog_cap: 8,
            },
        }
    }
}
impl crate::config::params::Params {
    pub fn palw_native_execution_active_at_v39(&self, daa: u64) -> bool {
        self.palw_native_execution_v39.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_native_final_active_at_v27(daa)
            && self.palw_funded_execution_active_at_v11(daa)
    }
    pub fn validate_palw_native_execution_v39(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error;
        let Some(f) = self.palw_native_execution_v39.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        let domain =
            crate::palw_attempt_v2::palw_network_domain_v2_for(self.net.to_string().as_bytes(), Some(self.genesis.hash)).as_bytes();
        f.policies.check(domain).map_err(|_| PalwModeV2Error::Invalid("native39 owning network/source/quota policy is invalid"))?;
        if self.has_isolated_native_calibration_v34() {
            if f.activation != ForkActivation::always() {
                return Err(PalwModeV2Error::Invalid("native39 isolated validation profile arms the owning lane from genesis only"));
            }
            return self
                .validate_isolated_native_calibration_v34()
                .map_err(|_| PalwModeV2Error::Invalid("native39 isolated original policies differ"));
        }
        // Shipping networks (testnet-12 DAA 14,000 release plan): the measured-value gate. The exact policies must match a pinned
        // record carrying the independent full-task court/Final/redemption and launch evidence and its review. EMPTY: refused.
        if !palw_native_execution_policies_measured_v39(&f.policies, PALW_NATIVE_EXECUTION_MEASURED_RECORDS_V39) {
            return Err(PalwModeV2Error::Invalid(
                "native39 shipping activation has no pinned measured-value record for these exact policies \
                 (PALW_NATIVE_EXECUTION_MEASURED_RECORDS_V39 is empty): independent full-task court/Final/redemption and launch \
                 evidence are required; it stays dormant",
            ));
        }
        Ok(())
    }
}

/// **A measured-value record for the owning39 execution policies** (by root, on their network) with the digests of the
/// independent full-task court/Final/redemption and launch evidence and of its review. Pinned only by a reviewed change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwNativeExecutionMeasuredRecordV39 {
    pub network_domain: Digest,
    pub policies_root: Digest,
    pub evidence_digest: [u8; 32],
    pub review_digest: [u8; 32],
    pub harness: &'static str,
}
/// **The pinned records — EMPTY.** Arming `palw_native_execution_v39` on a shipping network is refused.
pub const PALW_NATIVE_EXECUTION_MEASURED_RECORDS_V39: &[PalwNativeExecutionMeasuredRecordV39] = &[];
pub fn palw_native_execution_policies_measured_v39(
    policies: &NativeExecutionPoliciesV39,
    records: &[PalwNativeExecutionMeasuredRecordV39],
) -> bool {
    let root = policies.root();
    records.iter().any(|r| {
        r.network_domain == policies.network_domain
            && r.policies_root == root
            && r.evidence_digest != [0; 32]
            && r.review_digest != [0; 32]
            && r.evidence_digest != r.review_digest
    })
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeClassWireV39 {
    pub version: u16,
    pub source: Digest,
    pub policy: Digest,
    pub max_positions: u32,
    pub scope: Digest,
    pub resources: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeClassRecordV39 {
    pub version: u16,
    pub source: Digest,
    pub program: Digest,
    pub profile: Digest,
    pub inventory: Digest,
    pub real_parameters: Digest,
    pub input_declarations: Digest,
    pub original_task: Digest,
    pub original_context_max: u32,
    pub max_positions: u32,
    pub policy: Digest,
    pub scope: Digest,
    pub resources: Digest,
}
impl NativeClassRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-class/v39", self)
    }
}
/// Only the owning canonical source and independently derived21-axis maximum
/// can construct this token. Wire roots or an old class are not authority.
pub struct CheckedNativeClassV39 {
    wire: NativeClassWireV39,
    record: NativeClassRecordV39,
    derived: CheckedFlatResourcesV39,
    source: CheckedSourceV39,
}
impl CheckedNativeClassV39 {
    pub fn root(&self) -> Digest {
        self.record.root()
    }
    pub fn wire(&self) -> &NativeClassWireV39 {
        &self.wire
    }
    pub fn source(&self) -> &CheckedSourceV39 {
        &self.source
    }
    pub fn record(&self) -> &NativeClassRecordV39 {
        &self.record
    }
    pub fn scope(&self) -> &FlatCourtScopeV39 {
        self.derived.scope()
    }
    pub fn resources(&self) -> &FlatResourceVectorV39 {
        self.derived.vector()
    }
}
impl NativeClassWireV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 2 + 64 + 64 + 4 + 64 + 64 {
            return Err("native39 class exact fixed body extent".into());
        }
        let w: Self = borsh::from_slice(raw).map_err(|_| "native39 class exact grammar")?;
        if w.version != 39 || borsh::to_vec(&w).map_err(|e| e.to_string())? != raw {
            return Err("native39 class canonical version/body".into());
        }
        Ok(w)
    }
    pub fn derive(source: &CheckedSourceV39, policies: &NativeExecutionPoliciesV39, max_positions: u32) -> Result<Self, String> {
        policies.check(policies.network_domain)?;
        let resources = derive_flat_resources_v39(source, max_positions)?;
        Ok(Self {
            version: 39,
            source: source.root(),
            policy: policies.root(),
            max_positions,
            scope: resources.scope().root(),
            resources: resources.root(),
        })
    }
    pub fn check(&self, source: &CheckedSourceV39, policies: &NativeExecutionPoliciesV39) -> Result<CheckedNativeClassV39, String> {
        policies.check(policies.network_domain)?;
        let derived = derive_flat_resources_v39(source, self.max_positions)?;
        if self.version != 39
            || self.source != source.root()
            || self.policy != policies.root()
            || self.scope != derived.scope().root()
            || self.resources != derived.root()
        {
            return Err("native39 class differs from original source/program/real-PC/profile or derived maximum".into());
        }
        let record = NativeClassRecordV39 {
            version: 39,
            source: source.root(),
            program: source.program_root(),
            profile: source.profile_root(),
            inventory: source.inventory_root(),
            real_parameters: source.parameters().root(),
            input_declarations: source.input_declarations_root(),
            original_task: source.original_task_metadata(),
            original_context_max: source.original_context_max(),
            max_positions: self.max_positions,
            policy: policies.root(),
            scope: derived.scope().root(),
            resources: derived.root(),
        };
        Ok(CheckedNativeClassV39 { wire: self.clone(), record, derived, source: source.clone() })
    }
}
