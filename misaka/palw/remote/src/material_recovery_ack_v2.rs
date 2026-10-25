//! Durable provider ACK outbox. Submission does not retire an ACK: a funded
//! carrier's exact inclusion and native finality must be verified first.
use crate::monetary_material_gc::MonetaryGcCompletedV2;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    palw_material_release_recovery_v2::{PalwMaterialRecoveryActionV2, PalwMaterialTerminalReceiptV2, material_recovery_object_v2},
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1,
        preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::Transaction,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_ROWS: usize = 1;
const MAX_SIGNATURE: usize = 8192;
const MAX_TX: usize = 1 << 20;
const MAX_FILE: u64 = 32 << 20;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialRecoveryAckSubmissionV2 {
    pub transaction: Vec<u8>,
    pub accepted_block: Option<BlockHash>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialRecoveryAckRowV2 {
    pub root: Hash64,
    pub receipt: Hash64,
    pub provider: PalwBondKeyV2,
    pub signature: Vec<u8>,
    pub stream: Hash64,
    pub previous: Hash64,
    pub sequence: u64,
    pub stored_consents: Vec<Hash64>,
    pub submission: Option<MaterialRecoveryAckSubmissionV2>,
}
impl NativeHeapV1 for MaterialRecoveryAckSubmissionV2 {
    fn native_heap_v1(&self, p: &str) -> NativeCoverageV1 {
        let Self { transaction, accepted_block: _ } = self;
        transaction.native_heap_v1(p)
    }
}
impl NativeHeapV1 for MaterialRecoveryAckRowV2 {
    fn native_heap_v1(&self, p: &str) -> NativeCoverageV1 {
        let Self { root: _, receipt: _, provider: _, signature, stream: _, previous: _, sequence: _, stored_consents, submission } =
            self;
        signature.native_heap_v1(p).add::<Self>(stored_consents.native_heap_v1(p), p).add::<Self>(submission.native_heap_v1(p), p)
    }
}
impl NativeWireScanV1 for MaterialRecoveryAckSubmissionV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        Vec::<u8>::scan_native_wire_v1(r)?.merge(Option::<BlockHash>::scan_native_wire_v1(r)?)
    }
}
impl NativeWireScanV1 for MaterialRecoveryAckRowV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        Hash64::scan_native_wire_v1(r)?
            .merge(Hash64::scan_native_wire_v1(r)?)?
            .merge(PalwBondKeyV2::scan_native_wire_v1(r)?)?
            .merge(Vec::<u8>::scan_native_wire_v1(r)?)?
            .merge(Hash64::scan_native_wire_v1(r)?)?
            .merge(Hash64::scan_native_wire_v1(r)?)?
            .merge(u64::scan_native_wire_v1(r)?)?
            .merge(Vec::<Hash64>::scan_native_wire_v1(r)?)?
            .merge(Option::<MaterialRecoveryAckSubmissionV2>::scan_native_wire_v1(r)?)
    }
}
/// Fixed-size discovery metadata; signatures and raw transaction owners stay
/// behind their explicit pre-reserved accessors.
#[derive(Clone, Copy, Debug)]
pub struct MaterialRecoveryAckSummaryV2 {
    pub receipt: Hash64,
}
impl MaterialRecoveryAckRowV2 {
    /// Pre-admit the exact signature copy and two boxed enum owners before
    /// constructing/encoding, then decode into a separately retained typed owner.
    pub fn object_owned_v2(
        &self,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwConsensusObjectV2>, String> {
        use kaspa_consensus_core::palw_native_resource_v1::{reserve_native_requests_v1, retain_native_value_decode_v1};
        let requests = (self.signature.len() as u64)
            .checked_mul(4)
            .and_then(|n| {
                n.checked_add(
                    (std::mem::size_of::<PalwConsensusObjectV2>()
                        + std::mem::size_of::<PalwMaterialRecoveryActionV2>()
                        + std::mem::size_of::<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1>())
                        as u64
                        * 3,
                )
            })
            .ok_or("ACK object construction overflow")?;
        let _construction = reserve_native_requests_v1(requests).map_err(|e| e.to_string())?;
        let object = self.object();
        let encoded = borsh::to_vec(&object).map_err(|e| e.to_string())?;
        retain_native_value_decode_v1(&encoded, || borsh::from_slice::<PalwConsensusObjectV2>(&encoded)).map_err(|e| e.to_string())
    }
    pub fn transaction_owned_v2(
        &self,
    ) -> Result<Option<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Vec<u8>>>, String> {
        use kaspa_consensus_core::palw_native_resource_v1::{reserve_native_requests_v1, retain_native_value_decode_v1};
        let Some(s) = &self.submission else { return Ok(None) };
        let _copy = reserve_native_requests_v1(
            (s.transaction.len() as u64).checked_add(4).and_then(|n| n.checked_mul(2)).ok_or("ACK transaction ownership overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let encoded = borsh::to_vec(&s.transaction).map_err(|e| e.to_string())?;
        retain_native_value_decode_v1(&encoded, || borsh::from_slice::<Vec<u8>>(&encoded)).map(Some).map_err(|e| e.to_string())
    }
    pub fn object(&self) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::MonetaryMaterialActionV1 {
            action: Box::new(crate_action(self.root, self.receipt, self.stream, self.previous, self.sequence)),
            signer: self.provider,
            signature: self.signature.clone(),
        }
    }
}
fn crate_action(
    root: Hash64,
    receipt: Hash64,
    stream: Hash64,
    previous: Hash64,
    sequence: u64,
) -> kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1 {
    kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::RecoveryV2(Box::new(
        PalwMaterialRecoveryActionV2::Acknowledge { root, receipt, stream, previous, sequence },
    ))
}
#[derive(BorshSerialize, BorshDeserialize)]
struct Snapshot {
    version: u16,
    rows: BTreeMap<Hash64, MaterialRecoveryAckRowV2>,
}
impl NativeWireScanV1 for Snapshot {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        u16::scan_native_wire_v1(r)?.merge(BTreeMap::<Hash64, MaterialRecoveryAckRowV2>::scan_native_wire_v1(r)?)
    }
}
pub struct MaterialRecoveryAckOutboxV2 {
    _lease: File,
    _native_lease: Option<NativeDemandLeaseV1>,
    file: PathBuf,
    rows: BTreeMap<Hash64, MaterialRecoveryAckRowV2>,
}
impl MaterialRecoveryAckOutboxV2 {
    pub fn open(file: &Path) -> Result<Self, String> {
        let parent = file.parent().ok_or("ACK outbox lacks owned directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = file.file_name().ok_or("ACK outbox lacks filename")?.to_string_lossy();
        let lease = crate::monetary_material::acquire_provider_lease(&parent.join(format!("{name}.lock")))?;
        // The exclusive kernel lease rules out a concurrent writer. A pending
        // publication was never exposed as a usable outbox row; remove only our
        // exact numeric pending names, including an old process's crash debris.
        let prefix = format!("{name}.pending-");
        for entry in fs::read_dir(parent).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name().to_str().and_then(|n| n.strip_prefix(&prefix)).is_some_and(|s| s.parse::<u32>().is_ok()) {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        let (rows, native_lease) = match File::open(file) {
            Ok(source) => {
                let len = source.metadata().map_err(|e| e.to_string())?.len();
                if len > MAX_FILE {
                    return Err("recovery ACK outbox exceeds cap".into());
                }
                let _raw =
                    reserve_native_requests_v1(len.checked_add(1).and_then(|n| n.checked_mul(2)).ok_or("ACK raw workspace overflow")?)
                        .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                source.take(len + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                if bytes.len() as u64 != len {
                    return Err("ACK journal extent changed".into());
                }
                preflight(&bytes)?;
                let snapshot =
                    retain_native_value_decode_v1(&bytes, || borsh::from_slice::<Snapshot>(&bytes)).map_err(|e| e.to_string())?;
                if snapshot.version != 2 {
                    return Err("recovery ACK outbox version differs".into());
                }
                for (id, row) in &snapshot.rows {
                    if *id != row.receipt
                        || row.root == Hash64::default()
                        || row.signature.is_empty()
                        || row.stored_consents.is_empty()
                        || row.stored_consents.windows(2).any(|p| p[0] >= p[1])
                    {
                        return Err("recovery ACK outbox row differs".into());
                    }
                }
                let (snapshot, lease) = snapshot.into_parts_v1();
                (snapshot.rows, lease)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (BTreeMap::new(), None),
            Err(e) => return Err(e.to_string()),
        };
        Ok(Self { _lease: lease, _native_lease: native_lease, file: file.to_owned(), rows })
    }
    fn publish(&mut self, rows: BTreeMap<Hash64, MaterialRecoveryAckRowV2>, lease: Option<NativeDemandLeaseV1>) -> Result<(), String> {
        let bytes = borsh::to_vec(&(2u16, &rows)).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_FILE {
            return Err("recovery ACK outbox capacity exhausted before signing".into());
        }
        let parent = self.file.parent().ok_or("ACK outbox lacks owned directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = self.file.file_name().ok_or("ACK outbox lacks filename")?.to_string_lossy();
        let tmp = parent.join(format!("{name}.pending-{}", std::process::id()));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(|e| e.to_string())?;
        let result = (|| {
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&tmp, &self.file).map_err(|e| e.to_string())?;
            File::open(parent).and_then(|f| f.sync_all()).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result?;
        self.rows = rows;
        self._native_lease = lease;
        Ok(())
    }
    fn mutation_lease(&self, extra: u64) -> Result<Option<NativeDemandLeaseV1>, String> {
        let len = borsh::object_length(&(2u16, &self.rows)).map_err(|e| e.to_string())? as u64;
        let heap = self.rows.native_heap_v1("ACK mutation").bytes().ok_or("ACK map has unsupported native shape")?;
        reserve_native_requests_v1(
            heap.checked_mul(2)
                .and_then(|n| len.checked_mul(2).and_then(|m| n.checked_add(m)))
                .and_then(|n| extra.checked_mul(4).and_then(|m| n.checked_add(m)))
                .ok_or("ACK mutation ownership overflow")?,
        )
        .map_err(|e| e.to_string())
    }
    pub fn pending(&self) -> impl Iterator<Item = &MaterialRecoveryAckRowV2> {
        self.rows.values()
    }
    /// Caller obtained receipt from the native-finalized API/op242+native
    /// finality, and fsynced authority-index cleanup before this method. Every
    /// provider generation must have its exact durable deletion receipt.
    pub fn enqueue_after_durable_gc(
        &mut self,
        network: Hash64,
        receipt: &PalwMaterialTerminalReceiptV2,
        provider: PalwBondKeyV2,
        checkpoint: &kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialProviderAckCheckpointV2,
        completed: &[MonetaryGcCompletedV2],
        still_held: &[(Hash64, Hash64)],
        sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> Result<Hash64, String> {
        let id = receipt.id();
        if self.rows.contains_key(&id) {
            return Ok(id);
        }
        if self.rows.len() >= MAX_ROWS
            || !receipt.contract.generations.iter().any(|g| g.providers.contains(&provider))
            || receipt.acknowledged.contains(&provider)
        {
            return Err("recovery ACK is foreign, duplicate or outside capacity".into());
        }
        // One empty concrete map row proves its exact BTree node layout on
        // this std build. The transient generation set is scanned without any
        // allocation; dynamic signature/vector and enum owners are separate.
        let mut frame = [0u8; 4 + 64 + 64 + 64 + 68 + 4 + 64 + 64 + 8 + 4 + 1];
        frame[..4].copy_from_slice(&1u32.to_le_bytes());
        let node = preflight_native_value_v1::<BTreeMap<Hash64, MaterialRecoveryAckRowV2>>(frame.as_slice(), frame.len() as u64)
            .map_err(|e| e.to_string())?
            .logical_requests_v1();
        let n = u32::try_from(receipt.contract.generations.len()).map_err(|_| "ACK generation count overflow")?;
        let stream = std::io::Cursor::new(n.to_le_bytes()).chain(std::io::repeat(0).take(u64::from(n) * 64));
        let mut reader = NativeWireReaderV1::new(stream, 4 + u64::from(n) * 64);
        let set = BTreeSet::<Hash64>::scan_native_wire_v1(&mut reader).map_err(|e| e.to_string())?.heap;
        let fixed = (std::mem::size_of::<PalwConsensusObjectV2>()
            + std::mem::size_of::<PalwMaterialRecoveryActionV2>()
            + std::mem::size_of::<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1>())
            as u64;
        let extra = node
            .checked_add(set)
            .and_then(|v| v.checked_add(u64::from(n) * 64))
            .and_then(|v| v.checked_add(4 * MAX_SIGNATURE as u64))
            .and_then(|v| v.checked_add(4 * fixed))
            .ok_or("ACK construction bound overflow")?;
        let lease = self.mutation_lease(extra)?;
        let consents: BTreeSet<_> =
            receipt.contract.generations.iter().filter(|g| g.providers.contains(&provider)).map(|g| g.consent).collect();
        let root = receipt.contract.terms.root;
        if consents.is_empty()
            || still_held.iter().any(|(consent, _)| consents.contains(consent))
            || (!receipt.unused_native_session
                && consents.iter().any(|consent| {
                    !completed.iter().any(|r| {
                        r.intent.root == root
                            && r.intent.stored_consent == *consent
                            && r.intent.release_consent == *consent
                            && r.intent.tail == receipt.tail_block
                    })
                }))
        {
            return Err("ACK requires durable deletion of all provider generations".into());
        }
        if checkpoint.provider != provider || checkpoint.version != 2 {
            return Err("provider ACK checkpoint identity differs".into());
        }
        let stream = checkpoint.stream;
        let previous = checkpoint.last_receipt;
        let sequence = checkpoint.sequence.checked_add(1).ok_or("provider ACK sequence exhausted; explicit retirement required")?;
        let object = material_recovery_object_v2(
            network,
            PalwMaterialRecoveryActionV2::Acknowledge { root, receipt: id, stream, previous, sequence },
            provider,
            sign,
        )
        .ok_or("provider recovery ACK signer unavailable")?;
        let PalwConsensusObjectV2::MonetaryMaterialActionV1 { signature, .. } = object else { unreachable!() };
        if signature.len() > MAX_SIGNATURE {
            return Err("provider ACK signature exceeds codec bound".into());
        }
        let row = MaterialRecoveryAckRowV2 {
            root,
            receipt: id,
            provider,
            signature,
            stream,
            previous,
            sequence,
            stored_consents: consents.into_iter().collect(),
            submission: None,
        };
        let mut next = self.rows.clone();
        next.insert(id, row);
        self.publish(next, lease)?;
        Ok(id)
    }
    /// Persist the exact signed transaction BEFORE submission; retrying retains
    /// the same spend. A submitted carrier never implies chain acceptance.
    pub fn record_submitted(&mut self, receipt: Hash64, tx: &Transaction) -> Result<(), String> {
        let row = self.rows.get(&receipt).ok_or("unknown provider ACK")?;
        let object = row.object_owned_v2()?;
        let len = borsh::object_length(tx).map_err(|e| e.to_string())? as u64;
        if len > MAX_TX as u64 {
            return Err("ACK carrier exceeds cap".into());
        }
        let lease = self.mutation_lease(len)?;
        let payload = borsh::to_vec(&(kaspa_consensus_core::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2, &*object))
            .map_err(|e| e.to_string())?;
        if tx.payload != payload {
            return Err("ACK carrier substitutes signed recovery object".into());
        }
        let bytes = borsh::to_vec(tx).map_err(|e| e.to_string())?;
        let mut next = self.rows.clone();
        next.get_mut(&receipt).unwrap().submission =
            Some(MaterialRecoveryAckSubmissionV2 { transaction: bytes, accepted_block: None });
        self.publish(next, lease)
    }
    pub fn record_accepted(&mut self, receipt: Hash64, block: BlockHash) -> Result<(), String> {
        let lease = self.mutation_lease(0)?;
        let mut next = self.rows.clone();
        next.get_mut(&receipt).and_then(|r| r.submission.as_mut()).ok_or("ACK carrier was not durably submitted")?.accepted_block =
            Some(block);
        self.publish(next, lease)
    }
    /// Exact own stream/sequence/receipt PRESENT in native-finalized current
    /// state proves actual ACK application after old carrier/delta pruning.
    pub fn retire_checkpoint_finalized(
        &mut self,
        receipt: Hash64,
        mut verify: impl FnMut(&MaterialRecoveryAckRowV2) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let row = self.rows.get(&receipt).ok_or("unknown provider ACK")?;
        if !verify(row)? {
            return Ok(false);
        }
        let lease = self.mutation_lease(0)?;
        let mut next = self.rows.clone();
        next.remove(&receipt);
        self.publish(next, lease)?;
        Ok(true)
    }
    /// Return deletion receipts to retire only after verifying exact carrier
    /// inclusion AND native finality. Current receipt absence is insufficient,
    /// including when this provider was the last ACK and removed the receipt.
    pub fn retire_finalized(
        &mut self,
        receipt: Hash64,
        mut verify: impl FnMut(&PalwConsensusObjectV2, &[u8], BlockHash) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let row = self.rows.get(&receipt).ok_or("unknown provider ACK")?;
        let submission = row.submission.as_ref().ok_or("ACK has no submitted carrier")?;
        let block = submission.accepted_block.ok_or("ACK carrier inclusion is unknown")?;
        let object = row.object_owned_v2()?;
        if !verify(&object, &submission.transaction, block)? {
            return Ok(false);
        }
        let lease = self.mutation_lease(0)?;
        let mut next = self.rows.clone();
        next.remove(&receipt);
        self.publish(next, lease)?;
        Ok(true)
    }
}
// The persisted grammar has only byte vectors; walk all nested extents BEFORE
// Borsh. Transaction bytes remain opaque until the fee adapter's typed preflight.
fn preflight(bytes: &[u8]) -> Result<(), String> {
    struct C<'a>(&'a [u8]);
    impl<'a> C<'a> {
        fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
            let h = self.0.get(..n).ok_or("ACK snapshot truncated")?;
            self.0 = &self.0[n..];
            Ok(h)
        }
        fn count(&mut self, cap: usize) -> Result<usize, String> {
            let n = u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize;
            if n > cap { Err("ACK nested capacity exceeded".into()) } else { Ok(n) }
        }
    }
    let mut c = C(bytes);
    if u16::from_le_bytes(c.take(2)?.try_into().unwrap()) != 2 {
        return Err("ACK version differs".into());
    }
    let count = c.count(MAX_ROWS)?;
    let mut previous = None;
    for _ in 0..count {
        let key: [u8; 64] = c.take(64)?.try_into().unwrap();
        if previous.as_ref().is_some_and(|p| p >= &key) {
            return Err("ACK map repeats or reorders receipt".into());
        }
        previous = Some(key);
        c.take(64 + 64 + 68)?;
        let sig = c.count(MAX_SIGNATURE)?;
        c.take(sig)?;
        c.take(64 + 64 + 8)?;
        let n = c.count(64)?;
        c.take(n * 64)?;
        match c.take(1)?[0] {
            0 => {}
            1 => {
                let n = c.count(MAX_TX)?;
                c.take(n)?;
                match c.take(1)?[0] {
                    0 => {}
                    1 => {
                        c.take(64)?;
                    }
                    _ => return Err("ACK block option differs".into()),
                }
            }
            _ => return Err("ACK submission option differs".into()),
        }
    }
    if !c.0.is_empty() {
        return Err("ACK snapshot trailing bytes".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_material_release_recovery_v2::*,
        tx::{TransactionId, TransactionOutpoint},
    };
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }
    /// Storage/authority seam only: these signatures and finality callbacks do
    /// not stand for real native payment, a model execution or native finality.
    #[test]
    fn one_signed_ack_survives_reopen_and_retires_only_from_its_exact_current_checkpoint() {
        let dir = std::env::temp_dir().join(format!(
            "misaka-rfc08-ack-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let file = dir.join("provider.ack");
        let provider = bond(2);
        let consent = h(2);
        let root = h(1);
        let receipt = PalwMaterialTerminalReceiptV2 {
            contract: PalwMaterialRecoveryContractV2 {
                terms: PalwMaterialRecoveryTermsV2 {
                    version: 2,
                    root,
                    initial_consent: consent,
                    sponsor: bond(1),
                    nonce: h(3),
                    policy: h(4),
                    max_generations: 2,
                    all_generation_ack: true,
                },
                enrolled_daa: 1,
                generations: vec![PalwMaterialRecoveryGenerationV2 { consent, providers: vec![provider, bond(3)] }],
            },
            final_consent: consent,
            tail_block: h(5),
            release_block: h(6),
            unused_native_session: false,
            acknowledged: BTreeSet::new(),
        };
        let checkpoint = PalwMaterialProviderAckCheckpointV2 {
            version: 2,
            provider,
            stream: h(7),
            created_daa: 1,
            sequence: 0,
            reserved_acks: 1,
            reserved_answer_events: 0,
            last_receipt: Hash64::default(),
        };
        let completed = vec![MonetaryGcCompletedV2 {
            version: 2,
            intent: crate::monetary_material_gc::MonetaryGcIntentV1 {
                version: 1,
                stored_consent: consent,
                root,
                release_consent: consent,
                tail: receipt.tail_block,
                release_block: h(9),
            },
            native_claims: vec![],
        }];
        let mut outbox = MaterialRecoveryAckOutboxV2::open(&file).unwrap();
        assert!(outbox.enqueue_after_durable_gc(h(10), &receipt, provider, &checkpoint, &[], &[], |_, _, _| Some(vec![1])).is_err());
        let id =
            outbox.enqueue_after_durable_gc(h(10), &receipt, provider, &checkpoint, &completed, &[], |_, _, _| Some(vec![1])).unwrap();
        let mut other = receipt.clone();
        other.contract.terms.nonce = h(11);
        assert!(
            outbox
                .enqueue_after_durable_gc(h(10), &other, provider, &checkpoint, &completed, &[], |_, _, _| panic!(
                    "a second ACK must be refused BEFORE signing"
                ))
                .is_err()
        );
        drop(outbox);
        let mut outbox = MaterialRecoveryAckOutboxV2::open(&file).unwrap();
        assert_eq!(outbox.pending().count(), 1);
        assert!(!outbox.retire_checkpoint_finalized(id, |_| Ok(false)).unwrap());
        assert_eq!(outbox.pending().count(), 1);
        let mut current = checkpoint.clone();
        current.sequence = 1;
        current.reserved_acks = 0;
        current.last_receipt = id;
        assert_eq!(
            outbox
                .retire_checkpoint_finalized(id, |row| Ok(row.provider == current.provider
                    && row.stream == current.stream
                    && row.sequence == current.sequence
                    && row.receipt == current.last_receipt))
                .unwrap(),
            true
        );
        drop(outbox);
        assert_eq!(MaterialRecoveryAckOutboxV2::open(&file).unwrap().pending().count(), 0);
        let mut bomb = 2u16.to_le_bytes().to_vec();
        bomb.extend(2u32.to_le_bytes());
        assert!(preflight(&bomb).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unused_native_receipt_can_ack_without_a_bundle_but_cannot_ignore_held_bytes() {
        // Storage authority seam only: the real service obtains this positive
        // unused-session fact from a current native-finalized terminal row.
        use kaspa_consensus_core::palw_material_release_recovery_v2::*;
        let provider = bond(2);
        let consent = h(2);
        let receipt = PalwMaterialTerminalReceiptV2 {
            contract: PalwMaterialRecoveryContractV2 {
                terms: PalwMaterialRecoveryTermsV2 {
                    version: 2,
                    root: h(1),
                    initial_consent: consent,
                    sponsor: bond(1),
                    nonce: h(3),
                    policy: h(4),
                    max_generations: 1,
                    all_generation_ack: true,
                },
                enrolled_daa: 1,
                generations: vec![PalwMaterialRecoveryGenerationV2 { consent, providers: vec![provider, bond(3)] }],
            },
            final_consent: consent,
            tail_block: h(5),
            release_block: h(6),
            unused_native_session: true,
            acknowledged: BTreeSet::new(),
        };
        let checkpoint = PalwMaterialProviderAckCheckpointV2 {
            version: 2,
            provider,
            stream: h(7),
            created_daa: 1,
            sequence: 0,
            reserved_acks: 1,
            reserved_answer_events: 0,
            last_receipt: Hash64::default(),
        };
        let dir = std::env::temp_dir().join(format!(
            "misaka-unused-ack-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let mut outbox = MaterialRecoveryAckOutboxV2::open(&dir.join("ack")).unwrap();
        assert!(
            outbox
                .enqueue_after_durable_gc(h(9), &receipt, provider, &checkpoint, &[], &[(consent, h(1))], |_, _, _| panic!(
                    "held material must refuse before signing"
                ))
                .is_err()
        );
        outbox.enqueue_after_durable_gc(h(9), &receipt, provider, &checkpoint, &[], &[], |_, _, _| Some(vec![1])).unwrap();
        assert_eq!(outbox.pending().count(), 1);
        drop(outbox);
        fs::remove_dir_all(dir).unwrap();
    }
}
