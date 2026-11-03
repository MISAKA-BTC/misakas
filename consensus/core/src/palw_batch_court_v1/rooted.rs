//! Branch-retained Batch demand and its original's reserved opening slot.
use super::{BatchDemandV1, BatchRangeV1, MAX_BATCH_RANGES_V1};
use crate::palw_court_shared_tir_state_v1 as shared;
use crate::palw_native_resource_v1::wire::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, wire_bad};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_state_v2::{PalwBlockContextV2, PalwChainStateV2};
use crate::{BlockHash, Hash64};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RootedBatchDemandV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub ordinal: u64,
    pub notice_id: Hash64,
    pub opened_block: BlockHash,
    pub request_root: Hash64,
    pub demand: BatchDemandV1,
}

impl RootedBatchDemandV1 {
    pub(crate) fn key(&self) -> Key {
        Key::CourtBatchDemand(self.claim, self.ordinal)
    }

    pub(crate) fn material_until(&self) -> u64 {
        self.demand.case_window.material_until_daa
    }

    pub(crate) fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1
            || *key != self.key()
            || [self.claim, self.scope_id, self.notice_id, self.opened_block, self.request_root].contains(&Hash64::default())
            || self.demand.checked_request_root_v1().map_err(|err| format!("Batch demand: {err:?}"))? != self.request_root
        {
            return Err("rooted Batch demand version, key or request identity".into());
        }
        Ok(())
    }

    pub(crate) fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        let context = shared::context(state, self.claim)?;
        let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(&Key::CourtSharedQuery(self.notice_id)) else {
            return Err("Batch demand lost its accepted notice".into());
        };
        notice.check(&Key::CourtSharedQuery(self.notice_id))?;
        if notice.claim != self.claim
            || notice.scope_id != self.scope_id
            || notice.material_until_daa != self.material_until()
            || context.scope_id != self.scope_id
            || context.original_accepted_daa != self.demand.case_window.claim_accepted_daa
            || context.material_until_daa != self.material_until()
            || self.demand.response_deadline_daa > context.hard_daa
            || state.court_shared_initial_notice_daa_v1(self.claim) != Some(notice.opened_daa)
            || notice.opened_daa != self.demand.case_window.challenge_opened_daa
            || batch_case_id_v1(self.claim, self.scope_id, self.notice_id, notice.opened_block, notice.opened_daa)?
                != self.demand.case_id
        {
            return Err("Batch demand substitutes its original or accepted notice".into());
        }
        Ok(())
    }

    pub(crate) fn check_adoption(&self, state: &PalwChainStateV2, at: &PalwBlockContextV2, signed_bytes: u64) -> Result<(), String> {
        self.check(&self.key())?;
        self.check_context(state)?;
        if (self.opened_block, self.demand.opened_daa) != (at.block, at.daa_score) {
            return Err("Batch demand differs from the accepting point".into());
        }
        let service = state.court_service_state_v1().ok_or("Batch demand has no rooted service book")?;
        shared::context(state, self.claim)?.check_binding(service)?;
        let binding = service.claims.get(&self.claim).ok_or("Batch demand has no original binding")?;
        let packet_id = batch_open_packet_v1(self.claim, self.scope_id, self.ordinal)?.as_bytes();
        let packet = service.book.packet(&packet_id).ok_or("Batch demand has no reserved opening packet")?;
        if !binding.packet_ids.contains(&packet_id)
            || service.book.assigned_daa(&packet_id) != Some(at.daa_score)
            || packet.cost.bytes < signed_bytes
        {
            return Err("Batch demand opening lacks reserved original capacity".into());
        }
        Ok(())
    }
}

pub(crate) fn batch_case_id_v1(
    claim: Hash64,
    scope: Hash64,
    notice: Hash64,
    notice_block: BlockHash,
    notice_daa: u64,
) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/batch-case/v1",
        &(claim, scope, notice, notice_block, notice_daa),
    )
}

/// Reserve the opening slot by original and ordinal before the notice or
/// request bytes exist. The committed row binds the selected request to it.
pub(crate) fn batch_open_packet_v1(claim: Hash64, scope: Hash64, ordinal: u64) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-open/v1", &(claim, scope, ordinal))
}

impl NativeWireScanV1 for RootedBatchDemandV1 {
    fn scan_native_wire_v1<R: io::Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        if reader.short()? != 1 {
            return Err(wire_bad("unknown rooted Batch demand version"));
        }
        reader.skip(5 * 64 + 8)?;
        // Demand case, four clocks, opening/response, maximum bytes and part width.
        reader.skip(64 + 7 * 8 + 4)?;
        let count = usize::try_from(reader.word()?).map_err(|_| wire_bad("rooted Batch range count overflow"))?;
        if !(1..=MAX_BATCH_RANGES_V1).contains(&count) {
            return Err(wire_bad("rooted Batch demand range count"));
        }
        for _ in 0..count {
            reader.step()?;
            reader.skip(64 + 4)?;
            if reader.byte()? > 1 {
                return Err(wire_bad("rooted Batch authentication mode"));
            }
        }
        NativeWireShapeV1 { collection_entries: count as u64, ..Default::default() }
            .allocation(count as u64, std::mem::size_of::<BatchRangeV1>())
    }
}

impl crate::palw_native_resource_v1::NativeHeapV1 for RootedBatchDemandV1 {
    fn native_members_v1(&self) -> Option<u64> {
        Some(self.demand.ranges.len() as u64)
    }

    fn native_heap_v1(&self, component: &str) -> crate::palw_native_resource_v1::NativeCoverageV1 {
        use crate::palw_native_resource_v1::NativeCoverageV1;
        match self.demand.ranges.capacity().checked_mul(std::mem::size_of::<BatchRangeV1>()).and_then(|n| u64::try_from(n).ok()) {
            Some(bytes) => NativeCoverageV1::Bounded { bytes },
            None => NativeCoverageV1::overflow::<Self>(component),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_batch_court_v1::{BatchCourtWindowV1, BatchRangeAuthenticationV1};

    #[test]
    fn rooted_batch_request_round_trip_binds_key_and_bounded_wire() {
        let h = Hash64::from_u64_word;
        let mut demand = BatchDemandV1 {
            case_id: batch_case_id_v1(h(1), h(2), h(3), h(4), 20).unwrap(),
            case_window: BatchCourtWindowV1 {
                claim_accepted_daa: 10,
                challenge_opened_daa: 20,
                collect_until_daa: 1_000,
                material_until_daa: 11_000,
            },
            opened_daa: 30,
            response_deadline_daa: 40,
            maximum_bytes: 100,
            part_bytes: 16,
            ranges: vec![BatchRangeV1 { id: h(5), bytes: 99, authentication: BatchRangeAuthenticationV1::WholeRange }],
        };
        let row = RootedBatchDemandV1 {
            version: 1,
            claim: h(1),
            scope_id: h(2),
            ordinal: 7,
            notice_id: h(3),
            opened_block: h(6),
            request_root: demand.checked_request_root_v1().unwrap(),
            demand: demand.clone(),
        };
        assert_ne!(demand.case_id, batch_case_id_v1(h(1), h(2), h(3), h(9), 20).unwrap(),);
        let key = row.key();
        assert_eq!(borsh::to_vec(&key).unwrap()[0], 32);
        assert_eq!(borsh::to_vec(&Row::CourtBatchDemand(Box::new(row.clone()))).unwrap()[0], 39);
        let body = borsh::to_vec(&row).unwrap();
        let restored: RootedBatchDemandV1 = borsh::from_slice(&body).unwrap();
        assert_eq!(restored, row);
        assert!(restored.check(&key).is_ok());
        assert!(restored.check(&Key::CourtBatchDemand(row.claim, 8)).is_err());
        demand.ranges[0].bytes += 1;
        assert!(RootedBatchDemandV1 { demand, ..restored }.check(&key).is_err());
        let mut reader = NativeWireReaderV1::new(body.as_slice(), body.len() as u64);
        let shape = RootedBatchDemandV1::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
        assert_eq!(shape.collection_entries, 1);
        let mut malformed = body.clone();
        // One encoded range occupies 64 bytes for the id, four for its
        // length and one for its authentication mode.
        let count_at = malformed.len() - (64 + 4 + 1) - 4;
        malformed[count_at..count_at + 4].copy_from_slice(&((MAX_BATCH_RANGES_V1 + 1) as u32).to_le_bytes());
        assert!(borsh::from_slice::<RootedBatchDemandV1>(&malformed).is_err());
        let mut reader = NativeWireReaderV1::new(malformed.as_slice(), malformed.len() as u64);
        assert!(RootedBatchDemandV1::scan_native_wire_v1(&mut reader).is_err());
    }
}
