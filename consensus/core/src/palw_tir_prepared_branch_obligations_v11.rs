// Included in the sealed final view. Borrowed classification only: no wire,
// persisted whole-actor body, tariff, paid permit or enlarged signature scope.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum FutureArchiveParticipantPolicyV11 {
    StandingBondAndSignedOpeningCheckedAtActualParticipation,
}
/// The exact existing literal is exposed, not reinterpreted as consent to extra
/// Source16 archive storage, producer FP liability, or future people's duties.
pub(super) enum NativeFinalBranchScopeV11<'a> {
    ExistingSourceResponse {
        signer:PalwBondKeyV2,turn:u64,
        custody:&'a crate::palw_tir_public_custody_v1::PalwTirPublicCustodyContractV1,
    },
    /// All legal query paths consume capacity; this does not name their future
    /// challenger/owner or make the old producer a supplier of new materials.
    ProtocolSourceQueryCapacity {logical:Hash64},
    ProtocolRetentionCapacity {segment:Option<u64>},
    FamilyTargetTransfer {
        consent:&'a crate::palw_court_family_supplier_v1::SignedFamilySupplierV1,
        target:&'a crate::palw_court_family_supplier_v1::FamilySupplierTargetV1,
    },
    /// Capacity belongs to the signed family contract's existing material and
    /// metadata demand. It grants no separate human sharedbook operator duty.
    FamilyContractHoldingCapacity {
        consent:&'a crate::palw_court_family_supplier_v1::SignedFamilySupplierV1,
    },
}
/// No public fields/constructor, clone or serializer. Packet and actual slot
/// borrow the same final book; release/deadline are never substituted for slot.
pub(super) struct NativeFinalBranchObligationV11<'a> {
    packet:&'a ServicePacket,assigned_daa:u64,scope:NativeFinalBranchScopeV11<'a>,
}
impl<'a> NativeFinalBranchObligationV11<'a> {
    pub(super) fn packet_v11(&self)->&'a ServicePacket{self.packet}
    pub(super) fn actual_assigned_daa_v11(&self)->u64{self.assigned_daa}
    pub(super) fn scope_v11(&self)->&NativeFinalBranchScopeV11<'a>{&self.scope}
}
/// Exact descriptive operation counts for a successful traversal, excluding
/// caller callback work and tree-internal comparison counts. No price or charge.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub(super) struct NativeFinalBranchTraversalWorkV11 {
    pub original_branch_records:u64,pub family_target_records:u64,
    pub family_target_packet_ids:u64,pub family_packet_classifications:u64,
    pub emitted_packet_records:u64,pub ordered_index_queries:u64,
}
fn branch_work_add_v11(a:u64,b:u64)->Result<u64,RefusalV11>{
    a.checked_add(b).ok_or_else(||resource_v11(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<NativeFinalBranchTraversalWorkV11>("native branch traversal work")))
}
impl PreparedOriginalArchiveServiceViewV11<'_> {
    pub(super) fn future_participant_policy_v11(&self)->FutureArchiveParticipantPolicyV11 {
        FutureArchiveParticipantPolicyV11::StandingBondAndSignedOpeningCheckedAtActualParticipation
    }
    /// No allocation or caller assignment DTO. Each final-bound packet is
    /// visited once in native derivation order, then canonical target order,
    /// then native holding order. No wire/packet order or identity is changed.
    pub(super) fn visit_branch_obligations_v11<'a>(
        &'a self,mut visit:impl FnMut(NativeFinalBranchObligationV11<'a>)->Result<(),RefusalV11>,
    )->Result<NativeFinalBranchTraversalWorkV11,RefusalV11>{
        use crate::palw_funded_original_admission_v1::FundedBranchV1 as Branch;
        let mut work=NativeFinalBranchTraversalWorkV11::default();
        let emit=|id:&[u8;64],scope:NativeFinalBranchScopeV11<'a>,visit:&mut dyn FnMut(NativeFinalBranchObligationV11<'a>)->Result<(),RefusalV11>,work:&mut NativeFinalBranchTraversalWorkV11|{
            let packet=self.packet_v11(id)?;
            let assigned_daa=self.book.assigned_daa(id).ok_or_else(||invalid_v11("branch map final assignment missing"))?;
            if !(packet.release_daa..packet.service_before).contains(&assigned_daa){return Err(invalid_v11("branch map final assignment outside actual packet interval"));}
            // packet_v11: binding-set query + book-packet query; assigned_daa:
            // one assignment-map query. These are operation counts, not rates.
            work.emitted_packet_records=branch_work_add_v11(work.emitted_packet_records,1)?;
            work.ordered_index_queries=branch_work_add_v11(work.ordered_index_queries,3)?;
            visit(NativeFinalBranchObligationV11{packet,assigned_daa,scope})
        };
        let custody=self.original_custody_v11();
        for (branch,id) in &self.derivation.branches {
            let scope=match branch {
                Branch::SignerTurn{signer,turn}=>NativeFinalBranchScopeV11::ExistingSourceResponse{signer:*signer,turn:*turn,custody},
                Branch::SourceNode{logical}=>NativeFinalBranchScopeV11::ProtocolSourceQueryCapacity{logical:*logical},
                Branch::Retention=>NativeFinalBranchScopeV11::ProtocolRetentionCapacity{segment:None},
                Branch::RetentionSegmentV2{ordinal}=>NativeFinalBranchScopeV11::ProtocolRetentionCapacity{segment:Some(*ordinal)},
            };
            work.original_branch_records=branch_work_add_v11(work.original_branch_records,1)?;
            emit(id,scope,&mut visit,&mut work)?;
        }
        for family in self.family {
            let before=work.emitted_packet_records;
            // Native builder creates transfer packets from these exact target
            // lists, then holding packets with io==0. Use existing final-book
            // indexes; no packet-by-target nested membership scan/new index.
            for target in &family.row.targets {
                work.family_target_records=branch_work_add_v11(work.family_target_records,1)?;
                for id in &target.packets {
                    work.family_target_packet_ids=branch_work_add_v11(work.family_target_packet_ids,1)?;
                    emit(id,NativeFinalBranchScopeV11::FamilyTargetTransfer{consent:&family.row.consent,target},&mut visit,&mut work)?;
                }
            }
            for packet in &family.packets {
                work.family_packet_classifications=branch_work_add_v11(work.family_packet_classifications,1)?;
                if packet.cost.io==0 {
                    emit(&packet.id,NativeFinalBranchScopeV11::FamilyContractHoldingCapacity{consent:&family.row.consent},&mut visit,&mut work)?;
                }
            }
            if work.emitted_packet_records.checked_sub(before)!=Some(family.packets.len() as u64){
                return Err(invalid_v11("branch map native transfer/holding count differs from family universe"));
            }
        }
        Ok(work)
    }
}
