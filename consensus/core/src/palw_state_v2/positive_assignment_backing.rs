//! Concrete native archive data for a current-prefix assigned Case. No cache
//! status, boolean, signature-only verdict or arbitrary source creates authority.
use super::*;
use crate::tx::{Transaction, UtxoEntry};
use crate::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1},
    palw_positive_assignment_material_v2::AssignmentAdmissionMaterialV2,
    palw_positive_journal_chain_v2::CaseJournalPinV2,
};
use misaka_palw_positive_contract::{assignment::rows::AssignmentRow, identity::CaseId, rows::AcceptedOrigin};
use std::sync::Arc;

pub struct NativeAcceptedCaseCarriageV2 {
    transaction: Transaction,
    _transaction_lease: NativeDemandLeaseV1,
    entries: Vec<UtxoEntry>,
    fee: u64,
    origin: AcceptedOrigin,
    selected_parent: BlockHash,
    prefix_state: Option<Arc<crate::palw_native_resource_v1::NativeChainStateV1>>,
    _lease: NativeDemandLeaseV1,
}
impl NativeAcceptedCaseCarriageV2 {
    /// DATA constructor; production callers supply actual canonical archived
    /// carrier and revalidated populated inputs. No `Full=true` flag is stored.
    pub fn from_native_history_v2(
        transaction: Transaction,
        transaction_lease: NativeDemandLeaseV1,
        entries: Vec<UtxoEntry>,
        fee: u64,
        origin: AcceptedOrigin,
        selected_parent: BlockHash,
        lease: NativeDemandLeaseV1,
    ) -> Result<Self, NativeAssignedCaseBackingErrorV2> {
        if transaction.id().as_bytes() != origin.carrier_transaction.0 || transaction.inputs.len() != entries.len() {
            return Err(NativeAssignedCaseBackingErrorV2::Origin);
        }
        Ok(Self {
            transaction,
            _transaction_lease: transaction_lease,
            entries,
            fee,
            origin,
            selected_parent,
            prefix_state: None,
            _lease: lease,
        })
    }
    /// Attach an ACTUAL executed-prefix owner reconstructed by the private
    /// native delta loader. The data is independently scope/head checked by
    /// the Case factory; missing original boundary is never current-state data.
    pub fn with_native_prefix_state_v2(
        mut self,
        state: Arc<crate::palw_native_resource_v1::NativeChainStateV1>,
        expected_root: Hash64,
    ) -> Result<Self, NativeAssignedCaseBackingErrorV2> {
        if self.prefix_state.is_some() || state.state_root() != expected_root {
            return Err(NativeAssignedCaseBackingErrorV2::Origin);
        }
        self.prefix_state = Some(state);
        Ok(self)
    }
    pub fn prefix_state(&self) -> Result<&PalwChainStateV2, NativeAssignedCaseBackingErrorV2> {
        self.prefix_state.as_ref().map(|state| &***state).ok_or(NativeAssignedCaseBackingErrorV2::Unavailable)
    }
    pub fn transaction(&self) -> &Transaction {
        &self.transaction
    }
    pub fn entries(&self) -> &[UtxoEntry] {
        &self.entries
    }
    pub fn fee(&self) -> u64 {
        self.fee
    }
    pub fn origin(&self) -> AcceptedOrigin {
        self.origin
    }
    pub fn selected_parent(&self) -> BlockHash {
        self.selected_parent
    }
}
pub struct NativeCommittedCaseRecordV2 {
    raw: Vec<u8>,
    carriage: NativeAcceptedCaseCarriageV2,
}
impl NativeCommittedCaseRecordV2 {
    pub fn from_native_history_v2(raw: Vec<u8>, carriage: NativeAcceptedCaseCarriageV2) -> Self {
        Self { raw, carriage }
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn carriage(&self) -> &NativeAcceptedCaseCarriageV2 {
        &self.carriage
    }
}
/// Complete rooted Assignment control history (Open/Bind/Join/Cancel/etc.).
/// Each carriage retains its actual executed prefix, so role eligibility is
/// resolved as of the historical event rather than from final retired flags.
pub struct NativeCommittedAssignmentRecordV2 {
    raw: Vec<u8>,
    carriage: NativeAcceptedCaseCarriageV2,
}
impl NativeCommittedAssignmentRecordV2 {
    pub fn from_native_history_v2(raw: Vec<u8>, carriage: NativeAcceptedCaseCarriageV2) -> Self {
        Self { raw, carriage }
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn carriage(&self) -> &NativeAcceptedCaseCarriageV2 {
        &self.carriage
    }
}
/// Selected-parent archive held across the block fold.
#[derive(Clone)]
pub struct NativePositiveDecisionHistoryV2(pub Arc<NativeCommittedAssignedCaseV2>);

impl std::fmt::Debug for NativePositiveDecisionHistoryV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NativePositiveDecisionHistoryV2").field(&self.0.pin().case).finish()
    }
}

impl PartialEq for NativePositiveDecisionHistoryV2 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for NativePositiveDecisionHistoryV2 {}

impl NativeHeapV1 for NativePositiveDecisionHistoryV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        self.0.native_heap_v1(path)
    }
}

pub struct NativeCommittedAssignedCaseV2 {
    parent: BlockHash,
    parent_state_root: Hash64,
    assignment: AssignmentRow,
    material: AssignmentAdmissionMaterialV2,
    open: NativeCommittedAssignmentRecordV2,
    pin: CaseJournalPinV2,
    records: Vec<NativeCommittedCaseRecordV2>,
    assignment_records: Vec<NativeCommittedAssignmentRecordV2>,
    _records_lease: NativeDemandLeaseV1,
    _open_lease: NativeDemandLeaseV1,
}
impl NativeCommittedAssignedCaseV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn from_native_archive_v2(
        parent: BlockHash,
        parent_state_root: Hash64,
        assignment: AssignmentRow,
        material: AssignmentAdmissionMaterialV2,
        open: NativeCommittedAssignmentRecordV2,
        pin: CaseJournalPinV2,
        records: Vec<NativeCommittedCaseRecordV2>,
        assignment_records: Vec<NativeCommittedAssignmentRecordV2>,
        records_lease: NativeDemandLeaseV1,
        open_lease: NativeDemandLeaseV1,
    ) -> Self {
        Self {
            parent,
            parent_state_root,
            assignment,
            material,
            open_raw,
            open_carriage,
            pin,
            records,
            assignment_records,
            _records_lease: records_lease,
            _open_lease: open_lease,
        }
    }
    pub fn parent(&self) -> BlockHash {
        self.parent
    }
    pub fn parent_state_root(&self) -> Hash64 {
        self.parent_state_root
    }
    pub fn assignment(&self) -> &AssignmentRow {
        &self.assignment
    }
    pub fn material(&self) -> &AssignmentAdmissionMaterialV2 {
        &self.material
    }
    pub fn open_raw(&self) -> &[u8] {
        self.open.raw()
    }
    pub fn open_carriage(&self) -> &NativeAcceptedCaseCarriageV2 {
        self.open.carriage()
    }
    pub fn pin(&self) -> &CaseJournalPinV2 {
        &self.pin
    }
    pub fn records(&self) -> &[NativeCommittedCaseRecordV2] {
        &self.records
    }
    pub fn assignment_records(&self) -> &[NativeCommittedAssignmentRecordV2] {
        &self.assignment_records
    }
}
pub enum NativeAssignedCaseBackingV2 {
    /// This says only that the exact parent had no Case. A current-prefix Case
    /// requires a genuine earlier op9 and its unpublished Case record; an empty
    /// committed history cannot be inferred from this variant.
    AbsentAtParent {
        case: CaseId,
        parent: BlockHash,
        parent_state_root: Hash64,
    },
    Committed(NativeCommittedAssignedCaseV2),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeAssignedCaseBackingErrorV2 {
    Unavailable,
    Origin,
    MissingCommittedMaterial,
    Shape,
    NativeResource(crate::palw_native_resource_v1::NativeCoverageV1),
}
impl NativeHeapV1 for NativeAcceptedCaseCarriageV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { transaction, _transaction_lease: _, entries, fee: _, origin: _, selected_parent: _, prefix_state, _lease: _ } =
            self;
        transaction
            .native_heap_v1(path)
            .add::<Self>(entries.native_heap_v1(path), path)
            .add::<Self>(prefix_state.native_heap_v1(path), path)
    }
}
impl NativeHeapV1 for NativeCommittedCaseRecordV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { raw, carriage } = self;
        raw.native_heap_v1(path).add::<Self>(carriage.native_heap_v1(path), path)
    }
}
impl NativeHeapV1 for NativeCommittedAssignedCaseV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self {
            parent: _,
            parent_state_root: _,
            assignment,
            material,
            open_raw,
            open_carriage,
            pin: _,
            records,
            assignment_records,
            _records_lease: _,
            _open_lease: _,
        } = self;
        assignment
            .native_heap_v1(path)
            .add::<Self>(material.native_heap_v1(path), path)
            .add::<Self>(open.native_heap_v1(path), path)
            .add::<Self>(records.native_heap_v1(path), path)
            .add::<Self>(assignment_records.native_heap_v1(path), path)
    }
}
impl NativeHeapV1 for NativeAssignedCaseBackingV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        match self {
            Self::AbsentAtParent { case: _, parent: _, parent_state_root: _ } => NativeCoverageV1::zero(),
            Self::Committed(value) => value.native_heap_v1(path),
        }
    }
}

impl NativeHeapV1 for NativeCommittedAssignmentRecordV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { raw, carriage } = self;
        raw.native_heap_v1(path).add::<Self>(carriage.native_heap_v1(path), path)
    }
}

pub const ASSIGNMENT_CONTROL_BOUNDARY_DOMAIN_V2: &[u8] = b"misaka/positive/assignment-native-boundary/v2/r1";
/// Exact Borsh DATA recipe, 496 bytes. The subject is an Assignment, not a
/// synthetic Case. Ordered populated-input digest uses the shared native input
/// recipe; its reference and the entire envelope are rooted by A's frame hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AssignmentControlBoundaryV2 {
    pub version: u16,
    pub revision: u16,
    pub assignment: misaka_palw_positive_contract::assignment::AssignmentId,
    pub origin: AcceptedOrigin,
    pub selected_parent: misaka_palw_positive_contract::rows::NativeHash64,
    pub pre_event_root: misaka_palw_positive_contract::identity::Digest,
    pub before_delta_end: u64,
    pub populated_inputs: misaka_palw_positive_contract::identity::Digest,
    pub input_count: u32,
    pub fee: u64,
}
impl AssignmentControlBoundaryV2 {
    pub fn from_actual_data_v2(
        assignment: misaka_palw_positive_contract::assignment::AssignmentId,
        origin: AcceptedOrigin,
        selected_parent: BlockHash,
        pre_event_root: Hash64,
        before_delta_end: usize,
        entries: &[UtxoEntry],
        fee: u64,
    ) -> Result<Self, misaka_palw_positive_contract::ContractError> {
        use misaka_palw_positive_contract::{ContractError, identity::Digest, rows::NativeHash64};
        Ok(Self {
            version: 2,
            revision: 1,
            assignment,
            origin,
            selected_parent: NativeHash64(selected_parent.as_bytes()),
            pre_event_root: Digest(pre_event_root.as_bytes()),
            before_delta_end: u64::try_from(before_delta_end).map_err(|_| ContractError::Budget)?,
            populated_inputs: crate::palw_positive_assignment_case_material_v2::assigned_case_inputs_id_v2(
                crate::TransactionId::from_bytes(origin.carrier_transaction.0),
                entries,
            )?,
            input_count: u32::try_from(entries.len()).map_err(|_| ContractError::Budget)?,
            fee,
        })
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, misaka_palw_positive_contract::ContractError> {
        use misaka_palw_positive_contract::{ContractError, identity::Digest};
        if bytes.len() != misaka_palw_positive_contract::assignment::control_material::ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2 {
            return Err(ContractError::NonCanonical);
        }
        let value: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        if value.version != 2 || value.revision != 1 {
            return Err(ContractError::Version);
        }
        if value.input_count == 0 || value.pre_event_root == Digest([0; 64]) || value.populated_inputs == Digest([0; 64]) {
            return Err(ContractError::Identity);
        }
        Ok(value)
    }
    /// Borrow only the literal body from a checked control journal frame.
    /// This checks framing/subject/origin/op kind, not body semantics, original
    /// acceptance, signatures, custody, or the original policy's body budgets.
    /// Native must still retrieve and compare the actual accepted transaction.
    pub fn native_body_from_record_data_v2<'a>(
        record: &misaka_palw_positive_contract::assignment::journal::AssignmentJournalRecordViewV2<'a>,
        maximum_payload: u64,
    ) -> Result<(Self, &'a [u8]), misaka_palw_positive_contract::ContractError> {
        use misaka_palw_positive_contract::{
            ContractError,
            assignment::{
                control_material::AssignmentControlViewV2,
                journal::AssignmentJournalEventKindV2 as K,
                open_material::{AssignmentOpenLimitsV2, AssignmentOpenViewV2},
            },
        };
        let header = record.header();
        let envelope = AssignmentControlViewV2::decode(record.payload(), maximum_payload)?;
        let boundary = Self::decode(envelope.boundary())?;
        if boundary.assignment != header.assignment || boundary.origin != header.origin {
            return Err(ContractError::Identity);
        }
        let (operation, body) = match header.kind {
            K::Open => {
                let open = AssignmentOpenViewV2::decode(envelope.inner(), AssignmentOpenLimitsV2::within_payload(maximum_payload))?;
                (8, open.native_body())
            }
            K::JoinProver => (13, envelope.inner()),
            K::Cancel => (10, envelope.inner()),
            K::Expire => (11, envelope.inner()),
            K::BindResult => (9, envelope.inner()),
            K::Settlement => (12, envelope.inner()),
            // MetadataReceipt is a distinct typed payload. Its actual receipt
            // owner must map it to carriage; it is never treated as an op body.
            K::MetadataReceipt => return Err(ContractError::Scope),
        };
        if body.get(..4) != Some(&[2, 0, 1, 0]) || body.get(4) != Some(&operation) {
            return Err(ContractError::NonCanonical);
        }
        Ok((boundary, body))
    }
    /// Exact ordered original inputs and fee, using the shared native DATA
    /// recipe without manufacturing a Case subject for an unconsumed A.
    pub fn check_populated_data(
        &self,
        transaction: &Transaction,
        entries: &[UtxoEntry],
        fee: u64,
    ) -> Result<(), misaka_palw_positive_contract::ContractError> {
        crate::palw_positive_assignment_case_material_v2::check_native_populated_inputs_v2(
            self.origin,
            self.input_count,
            self.populated_inputs,
            self.fee,
            transaction,
            entries,
            fee,
        )
    }
    pub fn canonical_bytes(
        &self,
    ) -> Result<
        [u8; misaka_palw_positive_contract::assignment::control_material::ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2],
        misaka_palw_positive_contract::ContractError,
    > {
        use borsh::BorshSerialize;
        let mut bytes = [0; misaka_palw_positive_contract::assignment::control_material::ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2];
        let mut writer = &mut bytes[..];
        self.serialize(&mut writer).map_err(|_| misaka_palw_positive_contract::ContractError::NonCanonical)?;
        if !writer.is_empty() {
            return Err(misaka_palw_positive_contract::ContractError::NonCanonical);
        }
        Self::decode(&bytes)?;
        Ok(bytes)
    }
}
