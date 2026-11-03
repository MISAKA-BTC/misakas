//! Native capacity visitors for the A0 aux151 VALUE schema. Fixed signatures
//! and the large row enum are real inline memory even for a short ClaimIndex.
//! No private replay archive, old aux142 bytes, or saved Pass DTO is admitted.
use super::wire::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, wire_bad, wire_struct};
use super::{NativeCoverageV1, NativeHeapV1, native_enum, native_struct, native_tuple_struct};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    assignment::{AssignmentId, AssignmentRoleV2, rows::*},
    assignment::{funding_relay::*, funding_relay_rows::*},
    funding::*,
    identity::*,
    messages::*,
    native::NativeRowKey,
    rows::*,
};
use std::{
    io::{self, Read},
    mem::size_of,
};

/// One retained typed slot per aux entry, regardless of that variant's encoded
/// width. This term is separate from the already charged raw key/value Vecs.
pub(crate) const POSITIVE_ROW_RETAINED_TYPED_BYTES_V1: usize = size_of::<NativeRowValue>();
/// Audited logical decoder workspace: bounded wire buffers plus eight typed
/// value slots for Borsh outer/body/result moves and shape/identity validation,
/// and two typed keys. No encoded-size multiplier stands for a large enum.
/// This is a conservative structural reservation, not measured physical RSS
/// or a compiler-stack-frame guarantee. Decoders keep no unbounded collections.
pub(crate) const POSITIVE_ROW_DECODE_WORKSPACE_BYTES_V1: usize =
    MAX_NATIVE_ROW_BYTES + MAX_NATIVE_KEY_BYTES + 8 * size_of::<NativeRowValue>() + 2 * size_of::<NativeRowKey>();
/// Restore's independent (case, attempt) uniqueness index. One BTree node
/// envelope per input row bounds the small cross-row set before validation.
pub(crate) const POSITIVE_ROW_ATTEMPT_INDEX_BYTES_V1: usize = size_of::<super::typed::BTreeNodeEnvelopeV1<([u8; 64], u32), ()>>();
pub(crate) const POSITIVE_ROW_ASSIGNMENT_NONCE_INDEX_BYTES_V1: usize =
    size_of::<super::typed::BTreeNodeEnvelopeV1<([u8; 68], u64), ()>>();
pub(crate) const fn positive_aux_row_memory_bytes_v1() -> u64 {
    (POSITIVE_ROW_RETAINED_TYPED_BYTES_V1
        + POSITIVE_ROW_DECODE_WORKSPACE_BYTES_V1
        + POSITIVE_ROW_ATTEMPT_INDEX_BYTES_V1
        + POSITIVE_ROW_ASSIGNMENT_NONCE_INDEX_BYTES_V1) as u64
}

// Exhaustive field/variant visitors make a future shared-schema field addition
// fail compilation until its heap policy is reviewed. All current ownership is
// inline: the containing map/vector visitor charges size_of::<T>() separately.
native_tuple_struct!(Digest; inner);
wire_struct!(Digest; [u8; 64]);
native_tuple_struct!(ClaimId; inner);
wire_struct!(ClaimId; Digest);
native_tuple_struct!(CaseId; inner);
wire_struct!(CaseId; Digest);
native_tuple_struct!(ResultId; inner);
wire_struct!(ResultId; Digest);
native_tuple_struct!(WitnessId; inner);
wire_struct!(WitnessId; Digest);
native_tuple_struct!(RequestId; inner);
wire_struct!(RequestId; Digest);
native_tuple_struct!(PrincipalId; inner);
wire_struct!(PrincipalId; Digest);
native_tuple_struct!(PlanId; inner);
wire_struct!(PlanId; Digest);
native_tuple_struct!(EvidencePartId; inner);
wire_struct!(EvidencePartId; Digest);
native_tuple_struct!(FundingScopeId; inner);
wire_struct!(FundingScopeId; Digest);
native_tuple_struct!(FundingConsentId; inner);
wire_struct!(FundingConsentId; Digest);
native_tuple_struct!(NativeHash64; inner);
wire_struct!(NativeHash64; [u8; 64]);
native_struct!(ChainScope; network, ruleset, branch);
wire_struct!(ChainScope; Digest, Digest, Digest);
native_struct!(MinerResultCommit; version, revision, scope, job, execution, miner, registered_class, artifact, program, input, tokenizer, execution_rules, selector, generation_seed, temperature_q24, prefill, decode, max_context, output, dependencies, service_contract);
wire_struct!(MinerResultCommit; u16, u16, ChainScope, Digest, Digest, PrincipalId, Digest, Digest, Digest, Digest, Digest, Digest, Digest, [u8; 32], u32, u32, u32, u32, Digest, Digest, Digest);
native_struct!(PanelWitnessCommit; version, revision, scope, case, result, author, attempt, profile, plan, execution_record, provenance, candidate_output);
wire_struct!(PanelWitnessCommit; u16, u16, ChainScope, CaseId, ResultId, PrincipalId, u32, CheckProfile, PlanId, Digest, Digest, Digest);
native_struct!(AggregateBudget; rounds, unique_source_bytes, transmitted_bytes, additional_work, retained_bytes, retention_until, fee, witness_attempts);
wire_struct!(AggregateBudget; u64, u64, u64, u64, u64, u64, u64, u32);
native_struct!(MinerServiceContract; version, revision, case, source_catalog, provider_delegations, budget, funding_consent, deadline);
wire_struct!(MinerServiceContract; u16, u16, CaseId, Digest, Digest, AggregateBudget, Digest, u64);
native_struct!(CoverageDescriptor; scope, result, plan, profile, mandatory_relations, reserved_elements, reserved_terms, max_part_bytes);
wire_struct!(CoverageDescriptor; ChainScope, ResultId, PlanId, CheckProfile, u64, u64, u64, u64);
native_struct!(FundingPlanBasis; profile, derivation, mandatory_relations, source_catalog, resource_quote, policy, schedule, required_fee, required_capital);
wire_struct!(FundingPlanBasis; CheckProfile, Digest, u64, Digest, Digest, Digest, Digest, u64, u64);
native_struct!(FundingConsent; version, revision, case, scope, payer, max_fees, max_locked, funding_output_index, refund_payload, admission_expiry);
wire_struct!(FundingConsent; u16, u16, CaseId, FundingScopeId, PrincipalId, u64, u64, u32, Digest, u64);
native_struct!(NativeOutpoint; transaction, index);
wire_struct!(NativeOutpoint; NativeHash64, u32);
native_struct!(AcceptedOrigin; accepting_block, carrier_block, carrier_transaction, accepted_tx_ordinal, event_ordinal, accepted_daa, accepted_blue_score);
wire_struct!(AcceptedOrigin; NativeHash64, NativeHash64, NativeHash64, u32, u32, u64, u64);
native_struct!(AcceptedMaterial; origin, digest);
wire_struct!(AcceptedMaterial; AcceptedOrigin, Digest);
native_struct!(ResultRow; result, miner_key, miner_signature, origin, policy, fired_fork, funding_scope, dependencies, role_assignment);
wire_struct!(ResultRow; MinerResultCommit, [u8; PUBKEY_BYTES], [u8; SIGNATURE_BYTES], AcceptedOrigin, Digest, Digest, FundingScopeId, AcceptedMaterial, AcceptedMaterial);
native_struct!(ClaimIndexRow; claim, result, case);
wire_struct!(ClaimIndexRow; ClaimId, ResultId, CaseId);
native_struct!(AggregateUsage; rounds, unique_source_bytes, transmitted_bytes, additional_work, retained_bytes, fee, witness_attempts);
wire_struct!(AggregateUsage; u64, u64, u64, u64, u64, u64, u32);
native_struct!(CaseRow; case, result, service, coverage, status, miner_data, witness_status, positive_witness, usage, reservation, liability, maturity_daa, unresolved_fraud, last_event);
wire_struct!(CaseRow; CaseId, ResultId, MinerServiceContract, CoverageDescriptor, ClaimStatus, MinerDataStatus, WitnessStatus, Option<WitnessId>, AggregateUsage, AcceptedMaterial, AcceptedMaterial, u64, u32, AcceptedOrigin);
native_struct!(WitnessRow; witness, panel_key, panel_signature, origin, record_header, generated_tokens, role_assignment);
wire_struct!(WitnessRow; PanelWitnessCommit, [u8; PUBKEY_BYTES], [u8; SIGNATURE_BYTES], AcceptedOrigin, AcceptedMaterial, AcceptedMaterial, AcceptedMaterial);
native_struct!(CoverageRow; witness, case, result, plan, mandatory_relations, next_relation, frame_bytes, signatures, elements, terms, state, last_event);
wire_struct!(CoverageRow; WitnessId, CaseId, ResultId, PlanId, u64, u64, u64, u64, u64, u64, CoverageState, AcceptedOrigin);
native_struct!(SourceUnionRow; case, kind, source, source_units, requested_units, completed_units, union);
wire_struct!(SourceUnionRow; CaseId, SourceKind, Digest, u64, u64, u64, AcceptedMaterial);
native_struct!(ServiceReceiptRow; case, kind, source, unit, result, duty, response, provider, bytes, additional_work, settlement);
wire_struct!(ServiceReceiptRow; CaseId, SourceKind, Digest, u64, ResultId, AcceptedMaterial, AcceptedMaterial, PrincipalId, u64, u64, Option<AcceptedMaterial>);
native_struct!(BalanceLeg; initial, paid, refunded, burned, remaining);
wire_struct!(BalanceLeg; u64, u64, u64, u64, u64);
native_struct!(FundingRow; consent, basis, payer_key, payer_signature, funding_origin, original_outpoint, current_outpoint, initial_locked, fee, capital, surplus, liability, last_settlement);
wire_struct!(FundingRow; FundingConsent, FundingPlanBasis, [u8; PUBKEY_BYTES], [u8; SIGNATURE_BYTES], AcceptedOrigin, NativeOutpoint, Option<NativeOutpoint>, u64, BalanceLeg, BalanceLeg, BalanceLeg, AcceptedMaterial, Option<AcceptedMaterial>);
native_struct!(PacketReceiptRow; witness, first_relation, frame, author, signed_bytes, origin);
wire_struct!(PacketReceiptRow; WitnessId, u64, EvidencePartId, PrincipalId, u64, AcceptedOrigin);
native_struct!(JournalHeadRow; case, sequence, previous, head, history);
wire_struct!(JournalHeadRow; CaseId, u64, Digest, Digest, AcceptedMaterial);

macro_rules! row_wire_enum {
    ($ty:ty; $($tag:literal => ($($field:ty),*)),* $(,)?) => {
        impl NativeWireScanV1 for $ty {
            fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
                reader.step()?;
                match reader.byte()? {
                    $($tag => { #[allow(unused_mut)] let mut sum=NativeWireShapeV1::default();
                        $(sum=sum.merge(<$field as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;)* Ok(sum)
                    },)*
                    _ => Err(wire_bad("unallocated aux151 enum variant")),
                }
            }
        }
    };
}
native_enum!(CheckProfile; ExactStreamV1, FullRelationDirectV1);
row_wire_enum!(CheckProfile; 0=>(), 1=>());
native_enum!(SourceKind; ArtifactInventory, Input);
row_wire_enum!(SourceKind; 0=>(), 1=>());
native_enum!(MinerDataStatus; NotRequested, Pending, Complete);
row_wire_enum!(MinerDataStatus; 0=>(), 1=>(), 2=>());
native_enum!(WitnessStatus; Building, Committed, Checking, Complete, Rejected, Incomplete);
row_wire_enum!(WitnessStatus; 0=>(), 1=>(), 2=>(), 3=>(), 4=>(), 5=>());
native_enum!(ClaimStatus; ResultCommitted, Pending, PositivePassed, ClaimRefuted, EndedUnverified, Revoked);
row_wire_enum!(ClaimStatus; 0=>(), 1=>(), 2=>(), 3=>(), 4=>(), 5=>());
native_enum!(CoverageState; Checking, Matched, Mismatched, EndedUnverified);
row_wire_enum!(CoverageState; 0=>(), 1=>(), 2=>(), 3=>());
native_enum!(NativeRowKey; Result(x), ClaimIndex(x), Case(x), Witness(x), Coverage(x),
    SourceUnion{case,kind,source}, ServiceReceipt{case,kind,source,unit}, Funding(x),
    PacketReceipt{witness,first_relation}, JournalHead(x),Assignment(x),AssignmentNonce(x),AssignmentUse(x),AssignmentFunding(x),
    AssignmentSettlement{assignment,leg,party,ordinal},AssignmentRole{assignment,role,bond},AssignmentJournalHead(x),AssignmentJournalFrame{assignment,ordinal},AssignmentProverFunding{assignment,prover},FundingRelayAuthorization{funding,purpose},FundingRelayCustody{funding,purpose});
row_wire_enum!(NativeRowKey; 0=>(ResultId), 1=>(ClaimId), 2=>(CaseId), 3=>(WitnessId), 4=>(WitnessId),
    5=>(CaseId,SourceKind,Digest), 6=>(CaseId,SourceKind,Digest,u64), 7=>(CaseId), 8=>(WitnessId,u64), 9=>(CaseId),10=>(AssignmentId),11=>(NativeOutpoint),12=>(AssignmentId),13=>(AssignmentId),
    14=>(AssignmentId,AssignmentSettlementLegV2,PrincipalId,u64),15=>(AssignmentId,AssignmentRoleV2,NativeOutpoint),16=>(AssignmentId),17=>(AssignmentId,u64),18=>(AssignmentId,NativeOutpoint),19=>(NativeOutpoint,RelayFundingPurposeV2),20=>(NativeOutpoint,RelayFundingPurposeV2));
native_enum!(NativeRowBody; Result(x), ClaimIndex(x), Case(x), Witness(x), Coverage(x),
    SourceUnion(x), ServiceReceipt(x), Funding(x), PacketReceipt(x), JournalHead(x), Assignment(x),AssignmentNonce(x),AssignmentUse(x),AssignmentFunding(x),
    AssignmentSettlement(x),AssignmentRole(x),AssignmentJournalHead(x),AssignmentJournalFrame(x),AssignmentProverFunding(x),FundingRelayAuthorization(x),FundingRelayCustody(x));
row_wire_enum!(NativeRowBody; 0=>(ResultRow), 1=>(ClaimIndexRow), 2=>(CaseRow), 3=>(WitnessRow), 4=>(CoverageRow),
    5=>(SourceUnionRow), 6=>(ServiceReceiptRow), 7=>(FundingRow), 8=>(PacketReceiptRow), 9=>(JournalHeadRow),10=>(AssignmentRow),11=>(AssignmentNonceRow),12=>(AssignmentUseRow),13=>(AssignmentFundingRow),
    14=>(AssignmentSettlementRow),15=>(AssignmentRoleRow),16=>(AssignmentJournalHeadRow),17=>(AssignmentJournalFrameRow),18=>(AssignmentProverFundingRow),19=>(FundingRelayAuthorizationRowV2),20=>(FundingRelayCustodyRowV2));
native_struct!(NativeRowValue; version, revision, body);
impl NativeWireScanV1 for NativeRowValue {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        if reader.short()? != CONTRACT_VERSION || reader.short()? != CONTRACT_REVISION {
            return Err(wire_bad("unsupported aux151 VALUE version/revision"));
        }
        NativeRowBody::scan_native_wire_v1(reader)
    }
}

/// Full key-aware aux151 scan. The shared VALUE decoder owns row version,
/// kind, shape, signed-ID links and key/value identity. It verifies no signature,
/// accepted origin, native funding, computation or credit capability here.
/// Shared hashing streams canonical Borsh into the hasher, without Vec scratch.
pub(crate) fn scan_positive_aux_row_v1<R: Read>(key: &[u8], frame: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
    let n = usize::try_from(frame.remaining()).map_err(|_| wire_bad("aux151 frame exceeds address space"))?;
    if key.len() > MAX_NATIVE_KEY_BYTES || n > MAX_NATIVE_ROW_BYTES {
        return Err(wire_bad("aux151 key/value exceeds its fixed codec ceiling"));
    }
    frame.step()?;
    let mut bytes = [0u8; MAX_NATIVE_ROW_BYTES];
    frame.read_exact(&mut bytes[..n])?;
    let mut structural = NativeWireReaderV1::new(&bytes[..n], n as u64);
    let shape = NativeRowValue::scan_native_wire_v1(&mut structural)?;
    if structural.remaining() != 0 {
        return Err(wire_bad("aux151 VALUE has trailing bytes"));
    }
    let _checked = NativeRowValue::decode(key, &bytes[..n]).map_err(|_| wire_bad("aux151 key/value identity or shape mismatch"))?;
    // All terms are deliberately per entry, including short ClaimIndex rows.
    // The fixed DTO's own additional-heap visitor remains zero.
    shape
        .allocation(1, POSITIVE_ROW_RETAINED_TYPED_BYTES_V1)?
        .allocation(1, POSITIVE_ROW_DECODE_WORKSPACE_BYTES_V1)?
        .allocation(1, POSITIVE_ROW_ATTEMPT_INDEX_BYTES_V1)?
        .allocation(1, POSITIVE_ROW_ASSIGNMENT_NONCE_INDEX_BYTES_V1)
}

/// The generic aux map owns encoded Vecs, not typed VALUE slots. Reserve for
/// EVERY table151 entry before trying its decoder. A malformed row must not
/// earn a cheaper admission path; scanner/restore classify it separately.
fn positive_aux_coverage_v1(aux: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>, path: &str) -> (u64, NativeCoverageV1) {
    let count = aux.keys().filter(|(table, _)| *table == misaka_palw_positive_contract::native::KERNEL_AUX_TABLE).count();
    let Ok(count) = u64::try_from(count) else {
        return (u64::MAX, NativeCoverageV1::overflow::<NativeRowValue>(path));
    };
    let coverage = count
        .checked_mul(positive_aux_row_memory_bytes_v1())
        .map(|bytes| NativeCoverageV1::Bounded { bytes })
        .unwrap_or_else(|| NativeCoverageV1::overflow::<NativeRowValue>(path));
    (count, coverage)
}
impl NativeHeapV1 for crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { header, rows, aux, ledger_cache, positive_job_owners_v2, positive_gate_c_schedule_v2 } = self;
        let mut sum = NativeCoverageV1::zero();
        sum = sum.add::<Self>(header.native_heap_v1(path), path);
        sum = sum.add::<Self>(rows.native_heap_v1(path), path);
        sum = sum.add::<Self>(aux.native_heap_v1(path), path);
        sum = sum.add::<Self>(ledger_cache.native_heap_v1(path), path);
        sum = sum.add::<Self>(positive_job_owners_v2.native_heap_v1(path), path);
        sum = sum.add::<Self>(positive_gate_c_schedule_v2.native_heap_v1(path), path);
        sum.add::<Self>(positive_aux_coverage_v1(aux, path).1, path)
    }
    fn native_members_v1(&self) -> Option<u64> {
        let Self { header, rows, aux, ledger_cache, positive_job_owners_v2, positive_gate_c_schedule_v2 } = self;
        header
            .native_members_v1()?
            .checked_add(rows.native_members_v1()?)?
            .checked_add(aux.native_members_v1()?)?
            .checked_add(ledger_cache.native_members_v1()?)?
            .checked_add(positive_job_owners_v2.native_members_v1()?)?
            .checked_add(positive_gate_c_schedule_v2.native_members_v1()?)
    }
    fn native_components_v1(&self) -> Vec<super::NativeFieldInventoryV1> {
        let Self { header, rows, aux, ledger_cache, positive_job_owners_v2, positive_gate_c_schedule_v2 } = self;
        let derived = "PalwKernelRouteStateV1.aux151.typed-workspace";
        let (entries, heap) = positive_aux_coverage_v1(aux, derived);
        vec![
            super::NativeFieldInventoryV1::of("PalwKernelRouteStateV1.header", header),
            super::NativeFieldInventoryV1::of("PalwKernelRouteStateV1.rows", rows),
            super::NativeFieldInventoryV1::of("PalwKernelRouteStateV1.aux", aux),
            super::NativeFieldInventoryV1::of("PalwKernelRouteStateV1.ledger_cache", ledger_cache),
            super::NativeFieldInventoryV1::of("PalwKernelRouteStateV1.positive_job_owners_v2", positive_job_owners_v2),
            super::NativeFieldInventoryV1::of("PalwKernelRouteStateV1.positive_gate_c_schedule_v2", positive_gate_c_schedule_v2),
            super::NativeFieldInventoryV1 {
                component: derived.into(),
                native_type: std::any::type_name::<NativeRowValue>().into(),
                inline_bytes: 0,
                entries,
                heap,
            },
        ]
    }
}
