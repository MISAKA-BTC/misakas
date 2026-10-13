//! Readonly extraction of genuine native rows. This private checked projection
//! proves internal row agreement only; a node must anchor its state root to an
//! independently validated header, and UTXO redemption needs its own proof.
use borsh::BorshSerialize;
use misaka_palw_kernel::hash::{Digest, object_id};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeAuditRowV27 {
    pub table: u8,
    pub key: Vec<u8>,
    pub bytes: Vec<u8>,
}
impl NativeAuditRowV27 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/audit-exact-row/v27", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeExecutionAuditRecordV27 {
    pub version: u16,
    pub namespace: u16,
    pub network_domain: Digest,
    pub daa: u64,
    pub state_root: Digest,
    pub ledger_root: Digest,
    pub aux_root: Digest,
    pub class: Digest,
    pub source: Digest,
    pub program: Digest,
    pub real_parameter: Digest,
    pub inventory: Digest,
    pub job: Digest,
    pub funding: Digest,
    pub original_escrow_debit: u64,
    pub original_escrow_receipt: Digest,
    pub original_consent: Digest,
    pub original_final_policy: Digest,
    pub original_final_basis: Digest,
    pub maximum_scope: Digest,
    pub maximum_resource: Digest,
    pub actual_scope: Digest,
    pub actual_resource: Digest,
    pub claim: Digest,
    pub vector: Digest,
    pub trace: Digest,
    pub output: Digest,
    pub terminal_book: Digest,
    pub settlement: Digest,
    pub credit: Digest,
    pub producer_credit: u64,
    pub requester_operator: Digest,
    pub producer_operator: Digest,
    pub consumed_nonce: Digest,
    pub next_nonce: u64,
    pub established_daa: u64,
    pub payout_payload: Digest,
    pub payout_queue: Digest,
    /// A pooled queue may contain other jobs or already have drained. This is
    /// observation, not a per-job redeemed-UTXO assertion.
    pub observed_queue_amount: Option<u64>,
    pub rows: Vec<NativeAuditRowV27>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct CheckedNativeExecutionAuditViewV27 {
    record: NativeExecutionAuditRecordV27,
}
impl CheckedNativeExecutionAuditViewV27 {
    /// Only native Core extraction can mint this non-wire checked projection.
    pub(crate) fn from_native_rows(record: NativeExecutionAuditRecordV27) -> Result<Self, String> {
        if record.version != 27 || record.rows.len() > 16 || record.rows.iter().any(|r| r.bytes.len() > 1024 * 1024)
            || record.rows.iter().map(|r| r.bytes.len()).sum::<usize>() > 2 * 1024 * 1024
            || record.requester_operator == record.producer_operator || record.original_escrow_debit == 0
            || record.producer_credit != record.original_escrow_debit || record.original_final_policy == [0; 64]
        {
            return Err("native Final audit fixed authenticated row extent/conservation differs".into());
        }
        Ok(Self { record })
    }
    pub fn record(&self) -> &NativeExecutionAuditRecordV27 { &self.record }
    pub fn root(&self) -> Digest { object_id(b"misaka-palw/native/checked-final-row-view/v27", &self.record) }
}
