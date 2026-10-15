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
pub const NATIVE_ACTION_LAST_V39: u8 = 64;
pub const REGISTER_SOURCE_ACTION_V39: u8 = 41;
pub const REGISTER_CLASS_ACTION_V39: u8 = 42;
pub const SOURCE_START_ACTION_V39: u8 = 57;
pub const SOURCE_PART_ACTION_V39: u8 = 58;
pub const SOURCE_ASSEMBLE_ACTION_V39: u8 = 59;
pub const SOURCE_COLLECT_ACTION_V39: u8 = 60;
pub const SOURCE_PART_BYTES_V39: usize = 32 << 10;
pub const SOURCE_UPLOAD_MAX_BYTES_V39: usize = misaka_palw_kernel::flat_source39::SOURCE39_MAX_BYTES;

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
            SOURCE_PART_BYTES_V39 as u64,
            SOURCE_UPLOAD_MAX_BYTES_V39 as u64,
            misaka_palw_kernel::flat_resources39::ordinary_court_id_v39(),
            b"canonical39;real-PC-only;inputs-separate;original-context;paid-before-parse;owning-material61-64".to_vec(),
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
            || self.catalog_cap > 8
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
