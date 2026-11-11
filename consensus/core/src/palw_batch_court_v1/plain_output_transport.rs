//! Finite physical envelope for BCT2. Counts source carriers before any
//! court ruling or fee accounting. No shipped schedule supplies a budget.

use super::carriage::BATCH_PLAIN_OUTPUT_CARRIER_MAGIC_V2;
use crate::{
    config::params::ForkActivation, palw_court_service_v1::ServiceCost, subnets::SUBNETWORK_ID_PALW_LIFECYCLE, tx::Transaction,
};

// Upper envelope for UTXO/preflight, optional replay, observed and
// authoritative paths. Include separately bounded abandoned-prefix work.
pub const PLAIN_OUTPUT_NATIVE_PASSES_V2: u64 = 5;
pub const PLAIN_OUTPUT_ROLLBACK_PASSES_V2: u64 = 2;
pub const PLAIN_OUTPUT_PREFIX_OWNERS_V2: u64 = 3;
pub const PLAIN_OUTPUT_REPLAY_WORKSPACES_V2: u64 = 1;
// A terminal can copy the checked prefix and then either drop or restore it.
const PLAIN_OUTPUT_CHECKPOINT_VISITS_V2: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchPlainOutputTransportV2 {
    pub activation: ForkActivation,
    pub maximum_source_blocks: u64,
    pub maximum_source_controls: u64,
    pub maximum_work_per_pass: u64,
    pub maximum_io_per_pass: u64,
    pub maximum_workspace_per_carrier: u64,
    /// Byte bound for the retained PALW prefix before any BCT2 rehearsal.
    pub maximum_prefix_bytes: u64,
    /// Rate for the checkpoint copy and its disposal/undo over the typed prefix.
    pub maximum_prefix_work_per_byte: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchPlainOutputPhysicalBoundsV2 {
    pub controls: u64,
    pub work: u64,
    pub io: u64,
    pub memory: u64,
}

impl BatchPlainOutputTransportV2 {
    pub fn active_at(self, daa: u64) -> bool {
        self.activation != ForkActivation::never() && self.activation.is_active(daa)
    }

    /// Conservative sum, including malformed, duplicate, and refused
    /// carriers. Prefix memory includes both live and checkpoint owners.
    pub fn candidate_bounds_v2(self) -> Result<BatchPlainOutputPhysicalBoundsV2, &'static str> {
        if [
            self.maximum_source_blocks,
            self.maximum_source_controls,
            self.maximum_work_per_pass,
            self.maximum_io_per_pass,
            self.maximum_workspace_per_carrier,
            self.maximum_prefix_bytes,
            self.maximum_prefix_work_per_byte,
        ]
        .contains(&0)
        {
            return Err("BCT2 transport requires finite positive bounds");
        }
        let overflow = "BCT2 transport bound overflows";
        let controls = self.maximum_source_blocks.checked_mul(self.maximum_source_controls).ok_or(overflow)?;
        let attempts = controls.checked_mul(PLAIN_OUTPUT_NATIVE_PASSES_V2 + PLAIN_OUTPUT_ROLLBACK_PASSES_V2).ok_or(overflow)?;
        let checkpoint_bytes = attempts
            .checked_mul(PLAIN_OUTPUT_CHECKPOINT_VISITS_V2)
            .and_then(|visits| visits.checked_mul(self.maximum_prefix_bytes))
            .ok_or(overflow)?;
        let work = attempts
            .checked_mul(self.maximum_work_per_pass)
            .and_then(|work| work.checked_add(checkpoint_bytes.checked_mul(self.maximum_prefix_work_per_byte)?))
            .ok_or(overflow)?;
        let io = attempts.checked_mul(self.maximum_io_per_pass).and_then(|io| io.checked_add(checkpoint_bytes)).ok_or(overflow)?;
        let memory = controls
            .checked_add(PLAIN_OUTPUT_REPLAY_WORKSPACES_V2)
            .and_then(|count| count.checked_mul(self.maximum_workspace_per_carrier))
            .and_then(|bytes| bytes.checked_add(self.maximum_prefix_bytes.checked_mul(PLAIN_OUTPUT_PREFIX_OWNERS_V2)?))
            .ok_or(overflow)?;
        Ok(BatchPlainOutputPhysicalBoundsV2 { controls, work, io, memory })
    }

    pub(crate) fn admits_slot_v2(self, cost: ServiceCost) -> bool {
        cost.controls == 1 && cost.work != 0 && cost.work <= self.maximum_work_per_pass && cost.io <= self.maximum_io_per_pass
    }

    /// Quote the replay's concrete source, ID writers, and terminal evidence
    /// before loading any bodies. The checkpoint has a separate owner.
    pub(crate) fn reserve_replay_workspace_v2(
        self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        claim: crate::Hash64,
        terminal: bool,
    ) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_native_resource_v1::{
            NativeCoverageV1, NativeDecodeErrorV1, NativeHeapV1, TirNativeRequestsV1, native_logical_admission_enabled_v1,
            native_resource_admission_enabled_v1, reserve_native_pending_requests_v1,
        };
        use crate::palw_state_v2::PalwStateV2Error::{CarriageInconsistent as Refused, NativeResource};
        use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
            .ok_or_else(|| Refused("plain Output lacks its accepted frame".into()))?;
        let class = frame.original_v1().job.class_id;
        let record = state.tir_class_v1(&class).ok_or_else(|| Refused("plain Output lacks its accepted class".into()))?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class)
            .ok_or_else(|| Refused("plain Output lacks its accepted layout".into()))?;
        let source =
            PalwTirPublicSourceV1::native_construction_requests_v1(record, layout, frame.header_v1()).map_err(|why| match why {
                NativeDecodeErrorV1::Decode(why) => Refused(why),
                NativeDecodeErrorV1::Resource(why) => NativeResource(why),
            })?;
        let count = u64::from(frame.original_v1().decode_tokens_executed);
        let overflow = || NativeResource(NativeCoverageV1::overflow::<Self>("plain Output hash workspace"));
        let context_bytes = borsh::object_length(&frame.header_v1().job_context).map_err(|why| Refused(why.to_string()))? as u64;
        let id_bytes = count.checked_mul(std::mem::size_of::<u32>() as u64).ok_or_else(overflow)?;
        // Both canonical writers start at 512 and may own two buffers
        // during geometric growth. The Output stream writes two hashes,
        // a four-byte ID count and the exact token IDs.
        let writer_pair =
            |bytes: u64| bytes.max(512).checked_next_power_of_two().and_then(|capacity| capacity.checked_mul(2)).ok_or_else(overflow);
        let context_writer = writer_pair(context_bytes)?;
        let output_writer = writer_pair(id_bytes.checked_add(2 * 64 + 4).ok_or_else(overflow)?)?;
        // The borrowed Source and Output writers have no internal lease,
        // including when optional native admission is enabled.
        let mut scratch = TirNativeRequestsV1::new();
        scratch.add_v1(source).map_err(NativeResource)?;
        scratch.add_v1(context_writer.max(output_writer)).map_err(NativeResource)?;
        let unowned = scratch.bytes_v1();
        let mut total = TirNativeRequestsV1::new();
        total.add_v1(unowned).map_err(NativeResource)?;
        // The trace authentication has its own ID hash and context writer
        // scratch. It is separate from the retained authenticated ID Vec.
        total.add_v1(id_bytes).map_err(NativeResource)?;
        total.add_v1(context_writer).map_err(NativeResource)?;
        total.exact_v1::<u32>(count).map_err(NativeResource)?;
        if terminal {
            // Final settlement constructs a second Source, authenticates
            // its IDs and retains the rechecked evidence and copied frame.
            total.add_v1(source).map_err(NativeResource)?;
            total.exact_v1::<u32>(count).map_err(NativeResource)?;
            total.exact_v1::<u32>(count).map_err(NativeResource)?;
            total.exact_v1::<crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1>(1).map_err(NativeResource)?;
            total.exact_v1::<crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1>(1).map_err(NativeResource)?;
            let memory = frame.native_heap_v1("plain Output terminal frame");
            total.add_v1(memory.bytes().ok_or(NativeResource(memory))?).map_err(NativeResource)?;
        }
        let requests = total.bytes_v1();
        if requests > self.maximum_workspace_per_carrier {
            return Err(NativeResource(NativeCoverageV1::unsupported::<Self>(
                "plain Output replay workspace",
                "typed workspace exceeds its native bound",
            )));
        }
        // Legacy replay helpers lease their owned values only when an
        // optional native capability is armed. BCT2 always admits the
        // process workspace, keeping its scratch leased through replay.
        let outer = if native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1() { unowned } else { requests };
        reserve_native_pending_requests_v1(outer).map_err(NativeResource)
    }

    pub fn fingerprint_v2(self) -> crate::Hash64 {
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/plain-output-physical-transport/v2",
            &(
                self.activation.daa_score(),
                self.maximum_source_blocks,
                self.maximum_source_controls,
                self.maximum_work_per_pass,
                self.maximum_io_per_pass,
                self.maximum_workspace_per_carrier,
                self.maximum_prefix_bytes,
                self.maximum_prefix_work_per_byte,
                PLAIN_OUTPUT_CHECKPOINT_VISITS_V2,
                PLAIN_OUTPUT_NATIVE_PASSES_V2,
                PLAIN_OUTPUT_ROLLBACK_PASSES_V2,
                PLAIN_OUTPUT_PREFIX_OWNERS_V2,
                PLAIN_OUTPUT_REPLAY_WORKSPACES_V2,
            ),
        )
        .expect("fixed BCT2 transport policy")
    }

    pub fn candidate_meter_v2(self) -> Result<BatchPlainOutputSourceMeterV2, &'static str> {
        Ok(BatchPlainOutputSourceMeterV2 { remaining: self.candidate_bounds_v2()?.controls })
    }

    pub fn source_meter_v2(self) -> BatchPlainOutputSourceMeterV2 {
        BatchPlainOutputSourceMeterV2 { remaining: self.maximum_source_controls }
    }
}

pub struct BatchPlainOutputSourceMeterV2 {
    remaining: u64,
}

impl BatchPlainOutputSourceMeterV2 {
    pub fn charge_v2(&mut self, tx: &Transaction) -> Result<(), &'static str> {
        if tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && tx.payload.starts_with(BATCH_PLAIN_OUTPUT_CARRIER_MAGIC_V2) {
            self.remaining = self.remaining.checked_sub(1).ok_or("BCT2 source exceeds its physical carrier bound")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bct2_physical_meter_counts_duplicates_and_malformed_before_adjudication() {
        let policy = BatchPlainOutputTransportV2 {
            activation: ForkActivation::new(20),
            maximum_source_blocks: 3,
            maximum_source_controls: 2,
            maximum_work_per_pass: 3,
            maximum_io_per_pass: 4,
            maximum_workspace_per_carrier: 5,
            maximum_prefix_bytes: 10,
            maximum_prefix_work_per_byte: 2,
        };
        let bounds = policy.candidate_bounds_v2().unwrap();
        assert_eq!(bounds, BatchPlainOutputPhysicalBoundsV2 { controls: 6, work: 1806, io: 1008, memory: 65 });
        assert!(!policy.active_at(19));
        assert!(policy.active_at(20));
        let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_LIFECYCLE, 0, b"BCT2\xff".to_vec());
        let mut source = policy.source_meter_v2();
        source.charge_v2(&tx).unwrap();
        source.charge_v2(&tx).unwrap();
        assert!(source.charge_v2(&tx).is_err());
        assert!(source.charge_v2(&tx).is_err());
        let mut candidate = policy.candidate_meter_v2().unwrap();
        for _ in 0..bounds.controls {
            candidate.charge_v2(&tx).unwrap();
        }
        assert!(candidate.charge_v2(&tx).is_err());
        assert!(BatchPlainOutputTransportV2 { maximum_source_blocks: u64::MAX, ..policy }.candidate_bounds_v2().is_err());
    }
}
