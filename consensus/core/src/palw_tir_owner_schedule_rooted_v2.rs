/// Prospective rooted owner metadata for one immutable original frame. The
/// scheduler's lease never defines the retention or response clocks.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TirOwnerScheduleRowV2 {
    version: u16,
    claim: crate::Hash64,
    source: crate::Hash64,
    allocation: crate::Hash64,
    collect_until: u64,
    material_until: u64,
    schedule: OwnerScheduleV2<crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1, crate::Hash64>,
}

impl TirOwnerScheduleRowV2 {
    pub fn for_frame(
        frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
        roster_limit: usize,
        active_limit: usize,
        response_limit: usize,
        lease_daa: u64,
    ) -> Result<Self, String> {
        let plan = frame.plan_v1();
        let schedule = OwnerScheduleV2::new(roster_limit, active_limit, response_limit, lease_daa, plan.terms.accepted_daa)
            .map_err(|why| format!("invalid TIR owner scheduler limits: {why:?}"))?;
        let result = Self {
            version: 2,
            claim: plan.original_claim,
            source: plan.source_id,
            allocation: plan.allocation_id,
            collect_until: plan.terms.collect_until_daa,
            material_until: plan.terms.material_until_daa,
            schedule,
        };
        result.check(&result.key())?;
        Ok(result)
    }

    pub fn key(&self) -> crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 {
        crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirOwnerSchedule(self.claim)
    }

    pub(crate) fn material_until(&self) -> u64 {
        self.material_until
    }

    pub(crate) fn check(&self, key: &crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1) -> Result<(), String> {
        use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirAcquisition;
        if self.version != 2
            || *key != self.key()
            || self.claim == crate::Hash64::default()
            || self.source == crate::Hash64::default()
            || self.allocation == crate::Hash64::default()
            || self.collect_until > self.material_until
            || self.schedule.now >= self.material_until
        {
            return Err("TIR owner snapshot has invalid immutable frame scope".into());
        }
        for (i, row) in self.schedule.rows.iter().enumerate() {
            let TirAcquisition(claim, owner, allocation) = row.key else {
                return Err("TIR owner snapshot contains another key kind".into());
            };
            if claim != self.claim
                || allocation == crate::Hash64::default()
                || self
                    .schedule
                    .rows
                    .iter()
                    .take(i)
                    .any(|other| matches!(other.key, TirAcquisition(_, other_owner, _) if other_owner == owner))
                || matches!(row.phase, OwnerPhaseV2::Waiting { demand, collect_until, .. }
                    if demand == crate::Hash64::default() || collect_until != self.collect_until)
            {
                return Err("TIR owner snapshot substitutes frame, bond, demand or deadline".into());
            }
        }
        Ok(())
    }

    pub fn check_frame(&self, frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1) -> Result<(), String> {
        let plan = frame.plan_v1();
        self.check(&self.key())?;
        if self.claim != plan.original_claim
            || self.source != plan.source_id
            || self.allocation != plan.allocation_id
            || self.collect_until != plan.terms.collect_until_daa
            || self.material_until != plan.terms.material_until_daa
            || self.schedule.now < plan.terms.accepted_daa
        {
            return Err("TIR owner snapshot differs from current rooted frame".into());
        }
        Ok(())
    }

    /// Validate the persisted row against the assembled branch view. In-flight
    /// native responses cannot be reconstructed from scheduler metadata alone.
    pub(crate) fn check_context(&self, state: &crate::palw_state_v2::PalwChainStateV2) -> Result<(), String> {
        use crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::TirAcquisition;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, self.claim)
            .ok_or("TIR owner snapshot has no rooted original frame")?;
        self.check_frame(frame)?;
        if state.last_point().is_some_and(|point| self.schedule.now > point.daa_score) {
            return Err("TIR owner snapshot runs ahead of its branch clock".into());
        }
        if state.tir_owner_schedule_acquisitions_v2(self.claim).any(|key| self.schedule.phase(key).is_none()) {
            return Err("TIR owner snapshot omitted a rooted acquisition".into());
        }
        for row in &self.schedule.rows {
            let Some(TirAcquisition(owner)) = state.staged_court_row_v1(&row.key) else {
                return Err("TIR owner snapshot has no rooted acquisition".into());
            };
            self.check_owner(owner)?;
            if let OwnerPhaseV2::Waiting { turn, demand, respond_until, .. } = row.phase {
                let pending = owner.owned.active_v1().ok_or("TIR parked owner lost its rooted demand")?;
                if pending.turn != turn
                    || pending.demand_id != demand
                    || pending.respond_until_daa != respond_until
                    || !owner.owned.unpaid_v1()
                {
                    return Err("TIR parked owner substitutes its rooted response duty".into());
                }
            } else if owner.owned.unpaid_v1() && !(row.phase == OwnerPhaseV2::Closed && self.schedule.now >= self.collect_until) {
                return Err("TIR owner snapshot loses an outstanding response reservation".into());
            }
        }
        Ok(())
    }

    pub(crate) fn check_owner(&self, owner: &crate::palw_tir_public_state_v1::PalwTirPublicAcquisitionRowV1) -> Result<(), String> {
        let contract = owner.owned.contract_v1();
        if contract.claim != self.claim
            || contract.source_id != self.source
            || contract.bundle_id != self.allocation
            || contract.terms.collect_until_daa != self.collect_until
            || contract.terms.material_until_daa != self.material_until
        {
            return Err("TIR owner acquisition changes original scheduler scope".into());
        }
        Ok(())
    }

    /// Stage an already authenticated and funded native owner Open. A future
    /// writer must pair it with the Open's acquisition row. A shared RFC-0031
    /// notice alone creates no acquisition, owner slot, or response duty.
    pub fn stage_open(&self, owner: &crate::palw_tir_public_state_v1::PalwTirPublicAcquisitionRowV1) -> Result<Self, String> {
        self.check_owner(owner)?;
        let key = owner.key_v1()?;
        owner.check_structure_v1(&key)?;
        let contract = owner.owned.contract_v1();
        if owner.owned.active_v1().is_some() || owner.owned.spent_v1().last_progress_daa != contract.reserved_daa {
            return Err("TIR owner scheduler Open requires the initial acquisition row".into());
        }
        let mut next = self.clone();
        next.schedule.advance(contract.reserved_daa).map_err(|why| format!("TIR owner Open clock: {why:?}"))?;
        next.schedule.admit(key).map_err(|why| format!("TIR owner Open admission: {why:?}"))?;
        next.check(&next.key())?;
        Ok(next)
    }

    pub(crate) fn schedule(&self) -> &OwnerScheduleV2<crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1, crate::Hash64> {
        &self.schedule
    }

    pub(crate) fn with_schedule(
        &self,
        schedule: OwnerScheduleV2<crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1, crate::Hash64>,
    ) -> Result<Self, String> {
        let next = Self {
            version: self.version,
            claim: self.claim,
            source: self.source,
            allocation: self.allocation,
            collect_until: self.collect_until,
            material_until: self.material_until,
            schedule,
        };
        next.check(&next.key())?;
        Ok(next)
    }
}

impl BorshDeserialize for TirOwnerScheduleRowV2 {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let version = u16::deserialize_reader(reader)?;
        if version != 2 {
            return Err(invalid_snapshot("unknown TIR owner snapshot version"));
        }
        let result = Self {
            version,
            claim: crate::Hash64::deserialize_reader(reader)?,
            source: crate::Hash64::deserialize_reader(reader)?,
            allocation: crate::Hash64::deserialize_reader(reader)?,
            collect_until: u64::deserialize_reader(reader)?,
            material_until: u64::deserialize_reader(reader)?,
            schedule: OwnerScheduleV2::deserialize_reader(reader)?,
        };
        result.check(&result.key()).map_err(|why| io::Error::new(io::ErrorKind::InvalidData, why))?;
        Ok(result)
    }
}

impl crate::palw_native_resource_v1::NativeHeapV1 for TirOwnerScheduleRowV2 {
    fn native_members_v1(&self) -> Option<u64> {
        Some(self.schedule.rows.len() as u64)
    }

    fn native_heap_v1(&self, component: &str) -> crate::palw_native_resource_v1::NativeCoverageV1 {
        use crate::palw_native_resource_v1::NativeCoverageV1;
        use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1;
        let element = std::mem::size_of::<OwnerRow<PalwStagedCourtKeyV1, crate::Hash64>>();
        match self.schedule.rows.capacity().checked_mul(element).and_then(|n| u64::try_from(n).ok()) {
            Some(bytes) => NativeCoverageV1::Bounded { bytes },
            None => NativeCoverageV1::overflow::<Self>(component),
        }
    }
}

impl crate::palw_native_resource_v1::wire::NativeWireScanV1 for TirOwnerScheduleRowV2 {
    fn scan_native_wire_v1<R: io::Read>(
        reader: &mut crate::palw_native_resource_v1::wire::NativeWireReaderV1<R>,
    ) -> io::Result<crate::palw_native_resource_v1::wire::NativeWireShapeV1> {
        use crate::palw_native_resource_v1::wire::NativeWireShapeV1;
        use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1;
        use std::io::Read;
        reader.step()?;
        if reader.short()? != 2 {
            return Err(invalid_snapshot("unknown TIR owner snapshot version"));
        }
        reader.skip(3 * 64 + 2 * 8)?;
        let mut roster = [0; 8];
        reader.read_exact(&mut roster)?;
        let roster = u64::from_le_bytes(roster);
        reader.skip(4 * 8)?; // active cap, response cap, lease, observed clock
        let count = u64::from(reader.word()?);
        if roster == 0 || roster > MAX_ROSTER_V2 as u64 || count > roster {
            return Err(invalid_snapshot("TIR owner snapshot exceeds the roster ceiling"));
        }
        for _ in 0..count {
            reader.step()?;
            // The only allowed key is an acquisition (Borsh tag 5).
            if reader.byte()? != 5 {
                return Err(invalid_snapshot("TIR owner snapshot contains another key kind"));
            }
            reader.skip(64)?;
            crate::palw_state_v2::PalwBondKeyV2::scan_native_wire_v1(reader)?;
            reader.skip(64)?;
            match reader.byte()? {
                0 | 3 => {}
                1 => reader.skip(8)?,
                2 => reader.skip(64 + 3 * 8)?,
                _ => return Err(invalid_snapshot("invalid TIR owner snapshot phase")),
            }
            Option::<u64>::scan_native_wire_v1(reader)?;
        }
        NativeWireShapeV1 { collection_entries: count, ..Default::default() }
            .allocation(roster, std::mem::size_of::<OwnerRow<PalwStagedCourtKeyV1, crate::Hash64>>())
    }
}

#[cfg(test)]
mod rooted_tests {
    use super::*;
    use crate::{
        Hash64,
        palw_mode_v2::PalwConsensusMode,
        palw_native_resource_v1::wire::{NativeWireReaderV1, NativeWireScanV1},
        palw_staged_court_state_v1::PalwStagedCourtRowV1 as Row,
        palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwDeltaEntryV2, PalwStateDeltaV2, apply_delta_v2, revert_delta_v2},
    };

    #[test]
    fn native_scheduler_row_scans_and_replays_under_its_exact_key() {
        let f = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let schedule = TirOwnerScheduleRowV2::for_frame(&f.frame, 7, 2, 3, 4).unwrap();
        let key = schedule.key();
        let row = Row::TirOwnerSchedule(Box::new(schedule));
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(bytes[0], 37);
        assert_eq!(borsh::to_vec(&key).unwrap()[0], 30);
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = Row::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
        assert_eq!(
            shape.heap,
            crate::palw_native_resource_v1::NativeHeapV1::native_heap_v1(&row, "scheduler fixture").bytes().unwrap()
        );
        let decoded = borsh::from_slice::<Row>(&bytes).unwrap();
        assert_eq!(decoded, row);

        // Exercise the field-bearing phase and nested acquisition-key scanner
        // too. A real restored branch must separately supply that native row.
        let Row::TirOwnerSchedule(initial) = &row else { unreachable!() };
        let mut parked = (**initial).clone();
        let owner = crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(7), 0));
        let acquisition =
            crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirAcquisition(initial.claim, owner, Hash64::from_u64_word(11));
        parked.schedule.admit(acquisition).unwrap();
        parked
            .schedule
            .wait_for_response(acquisition, 1, Hash64::from_u64_word(13), parked.collect_until - 1, parked.collect_until)
            .unwrap();
        let parked = Row::TirOwnerSchedule(Box::new(parked));
        let parked_bytes = borsh::to_vec(&parked).unwrap();
        let mut reader = NativeWireReaderV1::new(parked_bytes.as_slice(), parked_bytes.len() as u64);
        assert_eq!(Row::scan_native_wire_v1(&mut reader).unwrap().collection_entries, 1);
        assert_eq!(reader.remaining(), 0);
        assert_eq!(borsh::from_slice::<Row>(&parked_bytes).unwrap(), parked);
        let mut too_many = bytes;
        let count = 1 + 2 + 3 * 64 + 2 * 8 + 5 * 8;
        too_many[count..count + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut reader = NativeWireReaderV1::new(too_many.as_slice(), too_many.len() as u64);
        assert!(Row::scan_native_wire_v1(&mut reader).is_err());
        assert!(borsh::from_slice::<Row>(&too_many).is_err());

        let PalwConsensusMode::ConsensusV2(bundle) = crate::config::params::palw_t12_shipped_params().palw_consensus_mode else {
            panic!("T12 fixture requires native state");
        };
        let base = PalwChainStateV2::genesis();
        let point = PalwBlockContextV2 {
            block: Hash64::from_u64_word(55),
            daa_score: f.frame.plan_v1().terms.accepted_daa,
            blue_score: 1,
            subsidy: 0,
        };
        let frame_key = f.frame.key_v1();
        let mut delta = PalwStateDeltaV2 {
            point,
            entries: vec![
                PalwDeltaEntryV2::StagedCourtSeed { key: frame_key, old: None, new: Some(Row::TirClaim(Box::new(f.frame).into())) },
                PalwDeltaEntryV2::StagedCourtSeed { key, old: None, new: Some(decoded) },
            ],
        };
        let child = apply_delta_v2(&base, &delta, &bundle.state).unwrap();
        assert_ne!(child.state_root(), base.state_root());
        assert_eq!(child.staged_court_row_v1(&key), Some(&row));
        let Some(Row::TirOwnerSchedule(schedule)) = child.staged_court_row_v1(&key) else { panic!("rooted schedule") };
        schedule.check_context(&child).unwrap();
        assert_eq!(revert_delta_v2(&child, &delta, &bundle.state).unwrap(), base);
        assert!(apply_delta_v2(&child, &delta, &bundle.state).is_err());

        // A valid parked-row codec is not permission to restore a response
        // reservation without its native acquisition in the same branch.
        let PalwDeltaEntryV2::StagedCourtSeed { new, .. } = &mut delta.entries[1] else { unreachable!() };
        *new = Some(parked);
        assert!(apply_delta_v2(&base, &delta, &bundle.state).is_err());
        *new = Some(row);
        let PalwDeltaEntryV2::StagedCourtSeed { key, .. } = &mut delta.entries[1] else { unreachable!() };
        *key = crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirOwnerSchedule(Hash64::from_u64_word(9));
        assert!(apply_delta_v2(&base, &delta, &bundle.state).is_err());
        assert!(base.staged_court_row_v1(&frame_key).is_none());
    }
}
