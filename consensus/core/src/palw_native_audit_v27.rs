//! Readonly extraction of genuine native rows. This private checked projection
//! proves internal row agreement only; a node must anchor its state root to an
//! independently validated header, and UTXO redemption needs its own proof.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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
        if record.version != 27
            || record.rows.len() > 16
            || record.rows.iter().any(|r| r.bytes.len() > 1024 * 1024)
            || record.rows.iter().map(|r| r.bytes.len()).sum::<usize>() > 2 * 1024 * 1024
            || record.requester_operator == record.producer_operator
            || record.original_escrow_debit == 0
            || record.producer_credit != record.original_escrow_debit
            || record.original_final_policy == [0; 64]
        {
            return Err("native Final audit fixed authenticated row extent/conservation differs".into());
        }
        Ok(Self { record })
    }
    pub fn record(&self) -> &NativeExecutionAuditRecordV27 {
        &self.record
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/checked-final-row-view/v27", &self.record)
    }
}

impl NativeExecutionAuditRecordV27 {
    /// Raw DTO only. Every dynamic count is bounded BEFORE allocating; a
    /// private proof32 checked factory independently reconstructs all fields.
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > 2 * 1024 * 1024 + 128 * 1024 {
            return Err("bounded native audit raw extent".into());
        }
        let mut r = std::io::Cursor::new(raw);
        let version = <u16>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let namespace = <u16>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let network_domain = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let daa = <u64>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let state_root = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let ledger_root = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let aux_root = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let class = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let source = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let program = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let real_parameter = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let inventory = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let job = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let funding = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let original_escrow_debit = <u64>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let original_escrow_receipt = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let original_consent = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let original_final_policy = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let original_final_basis = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let maximum_scope = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let maximum_resource = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let actual_scope = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let actual_resource = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let claim = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let vector = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let trace = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let output = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let terminal_book = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let settlement = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let credit = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let producer_credit = <u64>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let requester_operator = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let producer_operator = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let consumed_nonce = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let next_nonce = <u64>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let established_daa = <u64>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let payout_payload = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let payout_queue = <Digest>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let observed_queue_amount = <Option<u64>>::deserialize_reader(&mut r).map_err(|_| "bounded native audit fixed header")?;
        let count = u32::deserialize_reader(&mut r).map_err(|_| "bounded native audit row count")?;
        if count > 16 {
            return Err("bounded native audit row count exceeded".into());
        }
        let mut rows = Vec::with_capacity(count as usize);
        let mut total = 0usize;
        for _ in 0..count {
            let table = u8::deserialize_reader(&mut r).map_err(|_| "bounded native audit table")?;
            let key = bounded_audit_bytes(&mut r, 4101)?;
            let bytes = bounded_audit_bytes(&mut r, 1024 * 1024)?;
            total = total.checked_add(bytes.len()).filter(|n| *n <= 2 * 1024 * 1024).ok_or("bounded native audit row aggregate")?;
            rows.push(NativeAuditRowV27 { table, key, bytes });
        }
        if r.position() != raw.len() as u64 {
            return Err("bounded native audit trailing bytes".into());
        }
        Ok(Self {
            version,
            namespace,
            network_domain,
            daa,
            state_root,
            ledger_root,
            aux_root,
            class,
            source,
            program,
            real_parameter,
            inventory,
            job,
            funding,
            original_escrow_debit,
            original_escrow_receipt,
            original_consent,
            original_final_policy,
            original_final_basis,
            maximum_scope,
            maximum_resource,
            actual_scope,
            actual_resource,
            claim,
            vector,
            trace,
            output,
            terminal_book,
            settlement,
            credit,
            producer_credit,
            requester_operator,
            producer_operator,
            consumed_nonce,
            next_nonce,
            established_daa,
            payout_payload,
            payout_queue,
            observed_queue_amount,
            rows,
        })
    }
}
fn bounded_audit_bytes(r: &mut std::io::Cursor<&[u8]>, cap: usize) -> Result<Vec<u8>, String> {
    let n = u32::deserialize_reader(r).map_err(|_| "bounded native audit vector count")? as usize;
    let at = r.position() as usize;
    if n > cap || n > r.get_ref().len().saturating_sub(at) {
        return Err("bounded native audit vector extent".into());
    }
    let bytes = r.get_ref()[at..at + n].to_vec();
    r.set_position((at + n) as u64);
    Ok(bytes)
}

/// Unique original new27 credit. A later pooled fee/job payout cannot alias a
/// drained Final or manufacture its one-time UTXO redemption observation.
pub fn native_final_credit_queue_v27(job: Digest, funding: Digest, settlement: Digest, payout: crate::Hash64) -> crate::Hash64 {
    let mut id = object_id(b"misaka-palw/native/original-unique-final-credit-queue/v27", &(27u16, job, funding, settlement, payout));
    id[0] = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
    crate::Hash64::from_bytes(id)
}
