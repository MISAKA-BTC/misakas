//! A1's bounded delivery adapter for A0's primitive execution record. The
//! record carries roots/IDs only; registered program and job are taken from the
//! immutable, independently authenticated PreparedCheck, never from the peer.
//!
//! Every error in this module refuses only this delivery. It neither rejects a
//! signed witness candidate nor consumes an attempt, changes progress, convicts
//! a Miner or authorizes native admission/credit. A caller may retry the same
//! borrowed preparation after transport corruption.

use super::{AdapterError, CheckContext, PreparedCheck, digest};
use crate::{
    Hash64,
    palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirStepBindingV1},
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{PUBKEY_BYTES, SIGNATURE_BYTES},
    messages::PanelWitnessCommit,
    record::ExecutionRecord,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordLimits {
    pub max_record_bytes: u64,
    /// Logical native storage for the newly decoded Vec<u32> only. The retained
    /// program/job/plan copies and hashing/authentication/evaluation workspace
    /// need separate reservation; this is not a complete process/RSS limit.
    pub max_generated_heap_bytes: u64,
}

/// Convenience for callers already supplying one bounded transport cap. An
/// explicit RecordLimits can reserve generated storage independently of wire.
impl From<u64> for RecordLimits {
    fn from(cap: u64) -> Self {
        Self { max_record_bytes: cap, max_generated_heap_bytes: cap }
    }
}

/// Delivery-only diagnostic. None of these variants is a candidate verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordDeliveryError {
    Contract(ContractError),
    Adapter(AdapterError),
    Binding,
    Budget,
}
impl From<ContractError> for RecordDeliveryError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}
impl From<AdapterError> for RecordDeliveryError {
    fn from(error: AdapterError) -> Self {
        Self::Adapter(error)
    }
}

impl PreparedCheck {
    /// Decode one bounded primitive record and reconstruct its native binding
    /// from the class/job retained at authenticated preparation. No Result,
    /// class, layout, context or generated-count DTO is accepted from the peer.
    pub fn authenticate_record(
        &self,
        witness: PanelWitnessCommit,
        bytes: &[u8],
        limits: impl Into<RecordLimits>,
        panel_key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<CheckContext, RecordDeliveryError> {
        let limits = limits.into();
        let record = ExecutionRecord::decode(bytes, limits.max_record_bytes, self.result.decode)?;
        record.validate_binding(&self.result, &witness)?;

        // Both native values are private retained preparation inputs whose
        // program/layout/job passed successful preparation; no record-provided
        // program or native context is hashed or cloned here.
        // Check the stored Result binding again before copying any native data.
        let artifact = Hash64::from_bytes(self.result.artifact.0);
        if digest(self.native_class.class_id(&artifact)) != self.result.registered_class
            || digest(self.native_job.context_hash()) != self.result.execution
        {
            return Err(RecordDeliveryError::Binding);
        }

        let count = u64::from(record.generated_count());
        let generated_bytes = count.checked_mul(std::mem::size_of::<u32>() as u64).ok_or(RecordDeliveryError::Budget)?;
        if generated_bytes > limits.max_generated_heap_bytes {
            return Err(RecordDeliveryError::Budget);
        }
        let count = usize::try_from(count).map_err(|_| RecordDeliveryError::Budget)?;
        let mut generated = Vec::<u32>::new();
        generated.try_reserve_exact(count).map_err(|_| RecordDeliveryError::Budget)?;
        let capacity = u64::try_from(generated.capacity()).map_err(|_| RecordDeliveryError::Budget)?;
        if capacity.checked_mul(std::mem::size_of::<u32>() as u64).is_none_or(|bytes| bytes > limits.max_generated_heap_bytes) {
            return Err(RecordDeliveryError::Budget);
        }
        generated.extend(record.tokens());

        let header = record.header();
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: self.native_job.clone(),
            class: self.native_class.clone(),
            artifact_root: artifact,
            full_logits_trace_root: Hash64::from_bytes(header.full_logits_trace_root.0),
            step_leaf_count: header.step_leaf_count,
            step_merkle_root: Hash64::from_bytes(header.step_merkle_root.0),
            committed_execution_root: Hash64::from_bytes(header.committed_execution_root.0),
        };
        // Preserve borrowed preparation so every error remains retryable as a
        // delivery failure. The existing witness adapter performs signature,
        // plan, provenance, native binding and candidate-output checks.
        let prepared = PreparedCheck {
            result: self.result.clone(),
            plan: self.plan.clone(),
            descriptor: self.descriptor,
            form: self.form,
            limits: self.limits,
            native_class: self.native_class.clone(),
            native_job: self.native_job.clone(),
        };
        Ok(prepared.authenticate_witness(witness, binding, generated, panel_key, signature)?)
    }
}
