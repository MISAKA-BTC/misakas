//! Exhaustive native transaction allocation inventory. This is an allocation
//! bound for an already typed carrier, never script validation or provenance.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1};

impl NativeHeapV1 for ScriptPublicKey {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { version: _, script } = self;
        if !script.spilled() {
            return NativeCoverageV1::zero();
        }
        let Some(growth) = script.len().checked_mul(2) else { return NativeCoverageV1::overflow::<Self>(path) };
        NativeCoverageV1::Bounded { bytes: script.capacity().max(growth) as u64 }
    }
}
impl NativeHeapV1 for UtxoEntry {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { amount: _, script_public_key, block_daa_score: _, is_coinbase: _ } = self;
        script_public_key.native_heap_v1(path)
    }
}
impl NativeHeapV1 for TransactionInput {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { previous_outpoint: _, signature_script, sequence: _, sig_op_count: _ } = self;
        signature_script.native_heap_v1(path)
    }
}
impl NativeHeapV1 for TransactionOutput {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { value: _, script_public_key } = self;
        script_public_key.native_heap_v1(path)
    }
}
impl NativeHeapV1 for Transaction {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { version: _, inputs, outputs, lock_time: _, subnetwork_id: _, gas: _, payload, mass: _, id: _ } = self;
        inputs.native_heap_v1(path).add::<Self>(outputs.native_heap_v1(path), path).add::<Self>(payload.native_heap_v1(path), path)
    }
    fn native_members_v1(&self) -> Option<u64> {
        (self.inputs.len() as u64).checked_add(self.outputs.len() as u64)
    }
}
