//! Complete, unadopted private planning inputs. Parsing this declaration does
//! not select it for a network, arm a fork, reserve capacity or admit a claim.
//! Native work/IO counts are explicit; no conversion from the historical G14
//! session-weight estimate is inferred here.
use super::{ProtectedFundingTermsV3, ProtectedPlanningSchedulesV3};
use crate::{
    Hash64,
    palw_court_service_binding_v1::CourtServiceProfileV1,
    palw_court_service_v1::ServiceCost,
    palw_court_shared_material_intake_v1::batch::admission::{NativeBatchDeliveryCostsV1, NativeBatchDeliveryTimingV1},
    palw_court_shared_material_intake_v1::{MaterialIntakeScheduleV1, NumericalBatchReplayBudgetV1},
    palw_funded_original_coordinator_v1::FundedBatchScheduleV1,
    palw_tir_court_v1::PalwTirBuilderWorkLimitsV1,
    palw_tir_public_source_v1::parameter_cover_v1::{OriginalParameterCoverBudgetV1, StreamingParameterCoverBudgetV1},
};

macro_rules! declaration {
    ($visibility:vis struct $name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, borsh::BorshSerialize)]
        #[serde(deny_unknown_fields)]
        $visibility struct $name { $($field: $ty),* }
        crate::palw_native_resource_v1::native_struct!($name; $($field),*);
    };
}

declaration! {
    pub struct ProtectedPlanningDeclarationV3 {
        version: u16,
        service_book: BookV3,
        cover: CoverV3,
        batch: BatchV3,
        material: MaterialV3,
        funding: FundingV3,
        packet_carriage_sompi: u64,
        rounds: u64,
    }
}
declaration! { struct CostV3 { bytes: u64, work: u64, io: u64, controls: u64 } }
declaration! {
    struct BookV3 {
        budget: CostV3,
        packet_cost: CostV3,
        service_span_daa: u64,
    }
}
declaration! {
    struct CoverV3 {
        max_original_leaves: u64,
        max_total_edges: u64,
        max_total_coordinates: u64,
        max_root_memberships: u64,
        per_root_edges: u64,
        per_root_coordinates: u64,
        per_root_memberships: u64,
        maximum_ranges: u64,
        workspace_bytes: u64,
    }
}
declaration! {
    struct ReplayV3 {
        max_elements: u64,
        max_terms: u64,
        max_target_requests: u64,
        max_source_reads: u64,
        workspace_bytes: u64,
        max_fault_bytes: u64,
        base_work: u64,
        io: u64,
    }
}
declaration! {
    struct TimingV3 {
        open_daa: u64,
        provider_wait_daa: u64,
        begin_daa: u64,
        parts_daa: u64,
        finish_daa: u64,
        verification_daa: u64,
        stop_daa: u64,
        terminal_daa: u64,
    }
}
declaration! {
    struct BatchV3 {
        numerical_replay: ReplayV3,
        terminal: CostV3,
        recovery_daa: u64,
        maximum_packets: u64,
        timing: TimingV3,
        open_carrier: CostV3,
        begin_carrier: CostV3,
        part_carrier: CostV3,
        finish_carrier: CostV3,
        open_work_per_range: u64,
        finish_work_per_byte: u64,
        finish_io_per_byte: u64,
        stop_verification: CostV3,
        stop_filing: CostV3,
    }
}
declaration! { struct MaterialV3 { sompi_per_byte: u64, draft_ttl_daa: u64 } }
declaration! {
    struct FundingV3 {
        original_fee_sompi: u64,
        capital_floor_sompi: u64,
        maximum_dependencies: u64,
        maximum_material_visits: u64,
    }
}

fn finite(values: &[u64], positive: bool) -> Result<(), String> {
    if values.iter().any(|n| *n == u64::MAX || positive && *n == 0) {
        return Err("private planning declaration has absent or unbounded dimensions".into());
    }
    Ok(())
}

impl CostV3 {
    fn native(self) -> ServiceCost {
        ServiceCost { bytes: self.bytes, work: self.work, io: self.io, controls: self.controls }
    }
    fn validate(self, atomic: bool) -> Result<(), String> {
        finite(&[self.bytes, self.work, self.io, self.controls], false)?;
        if atomic && self.controls != 1 {
            return Err("private planning atomic cost must name one control".into());
        }
        Ok(())
    }
}

impl ReplayV3 {
    fn native(self) -> NumericalBatchReplayBudgetV1 {
        NumericalBatchReplayBudgetV1 {
            meter_checker_io: true,
            work: PalwTirBuilderWorkLimitsV1 {
                evaluation: misaka_palw_tir::demand::DemandLimits { max_elements: self.max_elements, max_terms: self.max_terms },
                max_target_requests: self.max_target_requests,
                max_source_reads: self.max_source_reads,
            },
            workspace_bytes: self.workspace_bytes,
            max_fault_bytes: self.max_fault_bytes,
            base_work: self.base_work,
            io: self.io,
        }
    }
}

impl TimingV3 {
    fn native(self) -> NativeBatchDeliveryTimingV1 {
        NativeBatchDeliveryTimingV1 {
            open_daa: self.open_daa,
            provider_wait_daa: self.provider_wait_daa,
            begin_daa: self.begin_daa,
            parts_daa: self.parts_daa,
            finish_daa: self.finish_daa,
            verification_daa: self.verification_daa,
            stop_daa: self.stop_daa,
            terminal_daa: self.terminal_daa,
        }
    }
}

impl ProtectedPlanningDeclarationV3 {
    pub const MAX_JSON_BYTES: usize = 16 * 1024;

    /// A complete bounded declaration: all fields are required, duplicate and
    /// unknown JSON fields are errors, and no pinned values are filled in.
    pub fn decode_json_v3(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > Self::MAX_JSON_BYTES {
            return Err("private planning declaration exceeds its byte bound".into());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        value.validate_v3()?;
        Ok(value)
    }

    /// Shape and arithmetic checks only. The full claim graph must still fit
    /// the reserved book, all notice/hard windows and actual material retention.
    pub fn validate_v3(&self) -> Result<(), String> {
        if self.version != 3 {
            return Err("unsupported private planning declaration version".into());
        }
        let c = self.cover;
        let b = self.batch;
        let r = b.numerical_replay;
        let t = b.timing;
        let f = self.funding;
        finite(&[self.service_book.packet_cost.bytes, self.service_book.packet_cost.work, self.service_book.packet_cost.io], true)?;
        finite(
            &[
                self.service_book.budget.bytes,
                self.service_book.budget.work,
                self.service_book.budget.io,
                self.service_book.budget.controls,
                self.service_book.service_span_daa,
                c.max_original_leaves,
                c.max_total_edges,
                c.max_total_coordinates,
                c.max_root_memberships,
                c.per_root_edges,
                c.per_root_coordinates,
                c.per_root_memberships,
                c.maximum_ranges,
                c.workspace_bytes,
                r.max_elements,
                r.max_terms,
                r.max_target_requests,
                r.max_source_reads,
                r.workspace_bytes,
                r.max_fault_bytes,
                r.base_work,
                r.io,
                b.recovery_daa,
                b.maximum_packets,
                t.open_daa,
                t.begin_daa,
                t.parts_daa,
                t.finish_daa,
                t.verification_daa,
                t.stop_daa,
                t.terminal_daa,
                self.material.sompi_per_byte,
                self.material.draft_ttl_daa,
                f.original_fee_sompi,
                f.capital_floor_sompi,
                f.maximum_dependencies,
                f.maximum_material_visits,
                self.packet_carriage_sompi,
                self.rounds,
            ],
            true,
        )?;
        finite(&[t.provider_wait_daa, b.open_work_per_range, b.finish_work_per_byte, b.finish_io_per_byte], false)?;
        self.service_book.packet_cost.validate(true)?;
        for cost in [b.terminal, b.open_carrier, b.begin_carrier, b.part_carrier, b.finish_carrier, b.stop_verification, b.stop_filing]
        {
            cost.validate(true)?;
        }
        if c.maximum_ranges > crate::palw_batch_court_v1::MAX_BATCH_RANGES_V1 as u64
            || c.per_root_edges > c.max_total_edges
            || c.per_root_coordinates > c.max_total_coordinates
            || c.per_root_memberships > c.max_root_memberships
            || usize::try_from(b.maximum_packets).is_err()
            || u32::try_from(self.rounds).is_err()
            || r.native().service_cost_v1().is_none()
            || !r.native().covers_terminal_recheck_v1(b.terminal.native())
        {
            return Err("private planning declaration has inconsistent native bounds".into());
        }
        super::protected_delivery_within_draft_v1(self.delivery_v3(), self.material.draft_ttl_daa)?;
        Ok(())
    }

    pub fn identity_v3(&self) -> Result<Hash64, String> {
        self.validate_v3()?;
        Ok(self.fingerprint_v3())
    }

    /// Publisher prompt identity for this exact prospective declaration. This
    /// shares the native resolver's funding preimages; it grants no activation,
    /// admission or authority to substitute a node's selected declaration.
    pub fn prompt_ruleset_v3(
        &self,
        encoding: misaka_palw_positive_contract::public_one_honest::PromptMaterialEncodingV1,
    ) -> Result<misaka_palw_positive_contract::identity::Digest, String> {
        Ok(encoding.ruleset_v1(misaka_palw_positive_contract::identity::Digest(self.schedules_v3()?.funded.profile_ids[0].as_bytes())))
    }

    /// Configuration identity includes even invalid declarations. A digest is
    /// not a shape check, capacity reservation or activation permit.
    pub fn fingerprint_v3(&self) -> Hash64 {
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/protected-planning-declaration/v3", self)
            .expect("fixed private planning declaration serialization")
    }

    /// Selected native material terms; validate the whole declaration first.
    pub fn material_schedule_v3(&self) -> Result<MaterialIntakeScheduleV1, String> {
        Ok(self.schedules_v3()?.material)
    }

    /// Exact service preimage owned by this declaration. A caller-supplied
    /// budget cannot bootstrap the protected namespace under its identity.
    pub fn service_book_profile_v3(&self) -> Result<CourtServiceProfileV1, String> {
        Ok(self.schedules_v3()?.service_book)
    }

    /// Batch replay uses the same service-book identity and replay budget as
    /// original planning. Selection alone supplies no approval or reservation.
    pub fn numerical_replay_profile_v3(
        &self,
    ) -> Result<crate::palw_court_shared_material_intake_v1::NumericalBatchReplayProfileV1, String> {
        let selected = self.schedules_v3()?;
        Ok(crate::palw_court_shared_material_intake_v1::NumericalBatchReplayProfileV1 {
            service_profile: crate::palw_court_service_state_v2::court_service_profile_id_v2(selected.service_book.id()),
            budget: selected.batch.delivery.numerical_verification,
        })
    }

    fn delivery_v3(&self) -> NativeBatchDeliveryCostsV1 {
        NativeBatchDeliveryCostsV1 {
            numerical_verification: self.batch.numerical_replay.native(),
            terminal: self.batch.terminal.native(),
            recovery_daa: self.batch.recovery_daa,
            maximum_packets: self.batch.maximum_packets as usize,
            timing: self.batch.timing.native(),
        }
    }

    /// Construct all protected inputs together. Only the later accepting owner
    /// can select this for a particular network/clock; this method has no side
    /// effects, no activation and no resource/capacity exemption.
    pub(crate) fn schedules_v3(&self) -> Result<ProtectedPlanningSchedulesV3, String> {
        let identity = self.identity_v3()?;
        let mut profile_ids = [Hash64::default(); 4];
        for (kind, id) in profile_ids.iter_mut().enumerate() {
            *id = crate::palw_court_shared_tir_stream_v1::stream_digest(
                b"misaka-palw/protected-private-funding/v3",
                &(identity, kind as u8),
            )?;
        }
        let c = self.cover;
        let cover = OriginalParameterCoverBudgetV1 {
            max_original_leaves: c.max_original_leaves,
            max_total_edges: c.max_total_edges,
            max_total_coordinates: c.max_total_coordinates,
            max_root_memberships: c.max_root_memberships,
        };
        Ok(ProtectedPlanningSchedulesV3 {
            service_book: CourtServiceProfileV1 {
                version: 1,
                budget: self.service_book.budget.native(),
                packet_cost: self.service_book.packet_cost.native(),
                packets_per_signer: self.rounds,
                service_span_daa: self.service_book.service_span_daa,
            },
            batch: FundedBatchScheduleV1 {
                cover: StreamingParameterCoverBudgetV1 {
                    total: cover,
                    per_root_edges: c.per_root_edges,
                    per_root_coordinates: c.per_root_coordinates,
                    per_root_memberships: c.per_root_memberships,
                    max_workspace_bytes: c.workspace_bytes,
                },
                maximum_ranges: c.maximum_ranges as usize,
                delivery: self.delivery_v3(),
                carrier_costs: [
                    self.batch.open_carrier.native(),
                    self.batch.begin_carrier.native(),
                    self.batch.part_carrier.native(),
                    self.batch.finish_carrier.native(),
                ],
                open_work_per_range: self.batch.open_work_per_range,
                finish_work_per_byte: self.batch.finish_work_per_byte,
                finish_io_per_byte: self.batch.finish_io_per_byte,
                stop_verification: self.batch.stop_verification.native(),
                stop_filing: self.batch.stop_filing.native(),
            },
            material: MaterialIntakeScheduleV1 {
                sompi_per_byte: self.material.sompi_per_byte,
                draft_ttl_daa: self.material.draft_ttl_daa,
                cover,
            },
            funded: ProtectedFundingTermsV3 {
                profile_ids,
                original_fee_sompi: self.funding.original_fee_sompi,
                capital_floor_sompi: self.funding.capital_floor_sompi,
                maximum_dependencies: self.funding.maximum_dependencies,
                maximum_material_visits: self.funding.maximum_material_visits,
            },
            packet_carriage_sompi: self.packet_carriage_sompi,
            rounds: self.rounds,
            cover_workspace_limit: Some(c.workspace_bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Schema exercise only: these are not calibrated capacities, fees, network
    // defaults or a successful claim quote. No fixture is admitted with them.
    fn declaration() -> ProtectedPlanningDeclarationV3 {
        let packet = CostV3 { bytes: 1, work: 1, io: 1, controls: 1 };
        let replay = ReplayV3 {
            max_elements: 1,
            max_terms: 1,
            max_target_requests: 1,
            max_source_reads: 1,
            workspace_bytes: 1024,
            max_fault_bytes: 1,
            base_work: 1,
            io: 1,
        };
        let cost = replay.native().service_cost_v1().unwrap();
        let terminal = CostV3 {
            bytes: cost.bytes.max(crate::palw_batch_court_v1::carriage::BATCH_NUMERICAL_TERMINAL_BYTES_V1),
            work: cost.work,
            io: cost.io,
            controls: 1,
        };
        ProtectedPlanningDeclarationV3 {
            version: 3,
            service_book: BookV3 { budget: terminal, packet_cost: packet, service_span_daa: 1 },
            cover: CoverV3 {
                max_original_leaves: 1,
                max_total_edges: 1,
                max_total_coordinates: 1,
                max_root_memberships: 1,
                per_root_edges: 1,
                per_root_coordinates: 1,
                per_root_memberships: 1,
                maximum_ranges: 1,
                workspace_bytes: 1024,
            },
            batch: BatchV3 {
                numerical_replay: replay,
                terminal,
                recovery_daa: 1,
                maximum_packets: 1,
                timing: TimingV3 {
                    open_daa: 1,
                    provider_wait_daa: 0,
                    begin_daa: 1,
                    parts_daa: 1,
                    finish_daa: 1,
                    verification_daa: 1,
                    stop_daa: 1,
                    terminal_daa: 1,
                },
                open_carrier: packet,
                begin_carrier: packet,
                part_carrier: packet,
                finish_carrier: packet,
                open_work_per_range: 2,
                finish_work_per_byte: 3,
                finish_io_per_byte: 4,
                stop_verification: packet,
                stop_filing: packet,
            },
            material: MaterialV3 { sompi_per_byte: 1, draft_ttl_daa: 4 },
            funding: FundingV3 { original_fee_sompi: 1, capital_floor_sompi: 2, maximum_dependencies: 3, maximum_material_visits: 4 },
            packet_carriage_sompi: 5,
            rounds: 1,
        }
    }

    #[test]
    fn complete_declaration_is_strict_and_constructs_all_selected_terms() {
        let declared = declaration();
        let bytes = serde_json::to_vec(&declared).unwrap();
        assert_eq!(ProtectedPlanningDeclarationV3::decode_json_v3(&bytes).unwrap(), declared);
        let selected = declared.schedules_v3().unwrap();
        assert_eq!(selected.service_book.budget, declared.service_book.budget.native());
        assert_eq!(selected.service_book.packet_cost, declared.service_book.packet_cost.native());
        assert_eq!(selected.service_book.service_span_daa, declared.service_book.service_span_daa);
        assert_eq!(selected.service_book.packets_per_signer, declared.rounds);
        assert_eq!(selected.batch.delivery, declared.delivery_v3());
        assert_eq!(selected.batch.cover.total, selected.material.cover);
        assert_eq!(selected.batch.cover.max_workspace_bytes, declared.cover.workspace_bytes);
        assert_eq!(selected.cover_workspace_limit, Some(declared.cover.workspace_bytes));
        assert_eq!(selected.batch.maximum_ranges as u64, declared.cover.maximum_ranges);
        assert_eq!(
            selected.batch.carrier_costs,
            [
                declared.batch.open_carrier.native(),
                declared.batch.begin_carrier.native(),
                declared.batch.part_carrier.native(),
                declared.batch.finish_carrier.native()
            ]
        );
        assert_eq!(
            (selected.batch.open_work_per_range, selected.batch.finish_work_per_byte, selected.batch.finish_io_per_byte),
            (2, 3, 4)
        );
        assert_eq!(selected.batch.stop_verification, declared.batch.stop_verification.native());
        assert_eq!(selected.batch.stop_filing, declared.batch.stop_filing.native());
        assert_eq!((selected.material.sompi_per_byte, selected.material.draft_ttl_daa), (1, 4));
        assert_eq!(
            (
                selected.funded.original_fee_sompi,
                selected.funded.capital_floor_sompi,
                selected.funded.maximum_dependencies,
                selected.funded.maximum_material_visits
            ),
            (1, 2, 3, 4)
        );
        assert_eq!((selected.packet_carriage_sompi, selected.rounds), (5, 1));
        let unique: std::collections::BTreeSet<_> = selected.funded.profile_ids.into_iter().collect();
        assert_eq!(unique.len(), 4);
        assert!(!unique.contains(&Hash64::default()));

        let value = serde_json::to_value(declared).unwrap();
        for group in ["service_book", "cover", "batch", "material", "funding"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(group);
            assert!(ProtectedPlanningDeclarationV3::decode_json_v3(&serde_json::to_vec(&missing).unwrap()).is_err());
            let mut unknown = value.clone();
            unknown[group]["implicit_default"] = serde_json::json!(1);
            assert!(ProtectedPlanningDeclarationV3::decode_json_v3(&serde_json::to_vec(&unknown).unwrap()).is_err());
        }
        let mut duplicate = String::from_utf8(bytes).unwrap();
        duplicate.insert_str(1, "\"version\":3,");
        assert!(ProtectedPlanningDeclarationV3::decode_json_v3(duplicate.as_bytes()).is_err());
        assert!(
            ProtectedPlanningDeclarationV3::decode_json_v3(&vec![b' '; ProtectedPlanningDeclarationV3::MAX_JSON_BYTES + 1]).is_err()
        );
    }

    #[test]
    fn declaration_identity_changes_with_terms_and_rejects_inconsistent_shapes() {
        let original = declaration();
        let first = original.schedules_v3().unwrap();
        for change in 0..6 {
            let mut next = original;
            match change {
                0 => next.service_book.budget.work += 1,
                1 => next.batch.finish_work_per_byte += 1,
                2 => next.material.draft_ttl_daa += 1,
                3 => next.funding.capital_floor_sompi += 1,
                4 => next.packet_carriage_sompi += 1,
                5 => next.cover.workspace_bytes += 1,
                _ => unreachable!(),
            }
            assert_ne!(next.identity_v3().unwrap(), original.identity_v3().unwrap());
            let selected = next.schedules_v3().unwrap();
            for i in 0..4 {
                assert_ne!(selected.funded.profile_ids[i], first.funded.profile_ids[i]);
            }
        }
        for change in 0..7 {
            let mut invalid = original;
            match change {
                0 => invalid.version = 4,
                1 => invalid.cover.maximum_ranges = crate::palw_batch_court_v1::MAX_BATCH_RANGES_V1 as u64 + 1,
                2 => invalid.batch.numerical_replay.max_terms = u64::MAX,
                3 => invalid.batch.terminal.work = 1,
                4 => invalid.material.draft_ttl_daa = 3,
                5 => invalid.funding.original_fee_sompi = 0,
                6 => invalid.batch.begin_carrier.controls = 2,
                _ => unreachable!(),
            }
            assert!(invalid.schedules_v3().is_err(), "invalid field group {change}");
        }
    }

    #[test]
    fn selected_terms_fail_closed_and_share_material_and_replay_preimages() {
        use crate::palw_court_shared_material_intake_v1::{MaterialIntakeMissingV1, material_intake_schedule_selected_v3};
        use crate::palw_native_resource_v1::NativeHeapV1;
        let original = declaration();
        let selected = ProtectedPlanningSchedulesV3::selected_v3(Some(original)).unwrap();
        assert_eq!(selected, original.schedules_v3().unwrap());
        assert_eq!(material_intake_schedule_selected_v3(true, Some(original)), Ok(selected.material));
        assert_eq!(material_intake_schedule_selected_v3(false, Some(original)), Err(MaterialIntakeMissingV1::Schedule));
        let replay = original.numerical_replay_profile_v3().unwrap();
        assert_eq!(replay.budget, selected.batch.delivery.numerical_verification);
        assert_eq!(
            replay.service_profile,
            crate::palw_court_service_state_v2::court_service_profile_id_v2(selected.service_book.id())
        );
        assert_eq!(original.native_heap_v1("planning"), crate::palw_native_resource_v1::NativeCoverageV1::zero());
        let mut changed = original;
        changed.material.sompi_per_byte += 1;
        assert_ne!(material_intake_schedule_selected_v3(true, Some(changed)).unwrap(), selected.material);
        let mut invalid = original;
        invalid.version = 4;
        assert!(ProtectedPlanningSchedulesV3::selected_v3(Some(invalid)).is_err());
        assert_eq!(material_intake_schedule_selected_v3(true, Some(invalid)), Err(MaterialIntakeMissingV1::Schedule));
        assert!(invalid.numerical_replay_profile_v3().is_err());
        assert_eq!(ProtectedPlanningSchedulesV3::selected_v3(None).unwrap(), ProtectedPlanningSchedulesV3::pinned_v3().unwrap());
    }

    #[test]
    fn pinned_funding_extraction_keeps_historical_terms() {
        let selected = ProtectedPlanningSchedulesV3::pinned_v3().unwrap();
        let funded = crate::palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1();
        assert_eq!(
            selected.funded.profile_ids,
            [funded.fee_profile_id(), funded.capital_profile_id(), funded.retention_profile_id(), funded.settlement_policy_id()]
        );
        assert_eq!(selected.funded.original_fee_sompi, funded.fee_sompi());
        assert_eq!(selected.funded.capital_floor_sompi, funded.capital_floor_sompi());
        assert_eq!(selected.funded.maximum_dependencies, funded.source_budget().max_queries);
        assert_eq!(selected.funded.maximum_material_visits, funded.source_budget().max_vertices);
        assert_eq!(selected.cover_workspace_limit, None);
    }
}
