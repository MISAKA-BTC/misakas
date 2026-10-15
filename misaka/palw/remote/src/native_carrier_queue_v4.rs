//! Bounded durable pending native carriers. A send does not retire a row; the
//! service supplies a positive native finalized application or a signed carrier
//! deadline beyond which this exact unaccepted action can never apply again.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1,
        NativeWireShapeV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::Transaction,
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
fn decode_owned<T: NativeWireScanV1, E: std::fmt::Display>(
    bytes: &[u8],
    decode: impl FnOnce() -> Result<T, E>,
) -> Result<NativeRetainedV1<T>, String> {
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(|e| e.to_string())?;
    retain_native_value_decode_v1(bytes, decode).map_err(|e| e.to_string())
}
const MAX_ROWS: usize = 4096;
const MAX_BYTES: u64 = 64 << 20;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeCarrierPurposeV4 {
    Enrollment { class: Hash64, executor: PalwBondKeyV2, root: Hash64, consent: Hash64, recovery: Hash64, until_daa: u64 },
    Answer { provider: PalwBondKeyV2, stream: Hash64, sequence: u64, event: Hash64, deadline_daa: u64 },
    Release { root: Hash64, consent: Hash64, tail: Hash64 },
}
impl NativeCarrierPurposeV4 {
    fn id(&self) -> Hash64 {
        match self {
            Self::Enrollment { root, .. } => *root,
            Self::Answer { event, .. } => *event,
            Self::Release { root, .. } => {
                Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/RELEASE-CARRIER/V4", root))
            }
        }
    }
    fn validate_object(&self, object: &PalwConsensusObjectV2) -> Result<(), String> {
        use kaspa_consensus_core::{
            palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2 as R,
            palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as M,
        };
        let PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, signer, .. } = object else {
            return Err("additional carrier is not monetary action".into());
        };
        let valid = match (self, &**action) {
            (Self::Enrollment { class, executor, root, consent, recovery, until_daa }, M::RecoveryV2(r)) => match &**r {
                R::FundedEnroll { monetary, terms, .. } => {
                    monetary.terms.root == *root
                        && monetary.terms.sponsor == *executor
                        && signer == executor
                        && monetary.id() == *consent
                        && terms.initial_consent == *consent
                        && Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/RELEASE-RECOVERY-CONSENT/V2", terms))
                            == *recovery
                        && monetary.terms.enrollment_until_daa == *until_daa
                        && monetary.terms.native_session.as_ref().is_some_and(|s| s.class_id == *class)
                }
                _ => false,
            },
            (Self::Answer { provider, stream, sequence, event, deadline_daa }, M::RecoveryV2(r)) => match &**r {
                R::AnswerAndCheckpoint { answer, stream: s, sequence: n, challenge_deadline_daa: d, .. } => {
                    signer == provider
                        && answer.provider() == *provider
                        && s == stream
                        && n == sequence
                        && d == deadline_daa
                        && Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                            b"MISAKA/PALW/MATERIAL-ANSWER-EVENT/V4",
                            &r.payload(),
                        )) == *event
                }
                _ => false,
            },
            (Self::Release { root, consent, tail }, M::Release { root: r, consent: c, tail_block: t }) => {
                root == r && consent == c && tail == t
            }
            _ => false,
        };
        if valid { Ok(()) } else { Err("native carrier immutable purpose/object binding differs".into()) }
    }
}
impl NativeHeapV1 for NativeCarrierPurposeV4 {
    fn native_heap_v1(&self, _: &str) -> NativeCoverageV1 {
        NativeCoverageV1::zero()
    }
}
impl NativeWireScanV1 for NativeCarrierPurposeV4 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0 => <(Hash64, PalwBondKeyV2, Hash64, Hash64, Hash64, u64)>::scan_native_wire_v1(r),
            1 => <(PalwBondKeyV2, Hash64, u64, Hash64, u64)>::scan_native_wire_v1(r),
            2 => <(Hash64, Hash64, Hash64)>::scan_native_wire_v1(r),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "native carrier purpose unknown")),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCarrierQueueRowV4 {
    pub purpose: NativeCarrierPurposeV4,
    pub object: Vec<u8>,
    pub submitted_transaction: Option<Vec<u8>>,
}
impl NativeHeapV1 for NativeCarrierQueueRowV4 {
    fn native_heap_v1(&self, p: &str) -> NativeCoverageV1 {
        let Self { purpose, object, submitted_transaction } = self;
        purpose.native_heap_v1(p).add::<Self>(object.native_heap_v1(p), p).add::<Self>(submitted_transaction.native_heap_v1(p), p)
    }
}
impl NativeWireScanV1 for NativeCarrierQueueRowV4 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        NativeCarrierPurposeV4::scan_native_wire_v1(r)?
            .merge(Vec::<u8>::scan_native_wire_v1(r)?)?
            .merge(Option::<Vec<u8>>::scan_native_wire_v1(r)?)
    }
}
impl NativeCarrierQueueRowV4 {
    pub fn object_owned(&self) -> Result<NativeRetainedV1<PalwConsensusObjectV2>, String> {
        decode_owned(&self.object, || borsh::from_slice::<PalwConsensusObjectV2>(&self.object)).map_err(|e| e.to_string())
    }
    pub fn transaction_owned(&self) -> Result<Option<NativeRetainedV1<Vec<u8>>>, String> {
        let Some(bytes) = &self.submitted_transaction else { return Ok(None) };
        let _raw = reserve_native_requests_v1(
            (bytes.len() as u64).checked_add(4).and_then(|n| n.checked_mul(2)).ok_or("pending transaction byte overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let encoded = borsh::to_vec(bytes).map_err(|e| e.to_string())?;
        decode_owned(&encoded, || borsh::from_slice::<Vec<u8>>(&encoded)).map(Some).map_err(|e| e.to_string())
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
struct Snapshot {
    version: u16,
    rows: BTreeMap<Hash64, NativeCarrierQueueRowV4>,
}
impl NativeWireScanV1 for Snapshot {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        u16::scan_native_wire_v1(r)?.merge(BTreeMap::<Hash64, NativeCarrierQueueRowV4>::scan_native_wire_v1(r)?)
    }
}
pub struct NativeCarrierQueueV4 {
    file: PathBuf,
    _file_lease: File,
    _native_lease: Option<NativeDemandLeaseV1>,
    rows: BTreeMap<Hash64, NativeCarrierQueueRowV4>,
}
impl NativeCarrierQueueV4 {
    pub fn open(file: &Path) -> Result<Self, String> {
        let parent = file.parent().ok_or("native carrier queue needs an owned directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = file.file_name().ok_or("native carrier queue filename absent")?.to_string_lossy();
        let lock = crate::monetary_material::acquire_provider_lease(&parent.join(format!("{name}.lock")))?;
        let (rows, lease) = match File::open(file) {
            Ok(source) => {
                let len = source.metadata().map_err(|e| e.to_string())?.len();
                if len > MAX_BYTES {
                    return Err("native carrier queue exceeds declared recovery cap".into());
                }
                let _raw = reserve_native_requests_v1(
                    len.checked_add(1).and_then(|n| n.checked_mul(2)).ok_or("native queue raw memory overflow")?,
                )
                .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                source.take(len + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                if bytes.len() as u64 != len {
                    return Err("native queue grew while opening".into());
                }
                let retained = decode_owned(&bytes, || borsh::from_slice::<Snapshot>(&bytes)).map_err(|e| e.to_string())?;
                if retained.version != 4 || retained.rows.len() > MAX_ROWS {
                    return Err("native queue version/count differs".into());
                }
                struct Compare<'a>(&'a [u8]);
                impl Write for Compare<'_> {
                    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                        if !self.0.starts_with(b) {
                            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "native queue is not canonical"));
                        }
                        self.0 = &self.0[b.len()..];
                        Ok(b.len())
                    }
                    fn flush(&mut self) -> std::io::Result<()> {
                        Ok(())
                    }
                }
                let mut canonical = Compare(&bytes);
                borsh::to_writer(&mut canonical, &*retained).map_err(|e| e.to_string())?;
                if !canonical.0.is_empty() {
                    return Err("native queue canonical extent differs".into());
                }
                for (id, row) in &retained.rows {
                    if *id != row.purpose.id() {
                        return Err("native carrier key differs from immutable purpose".into());
                    }
                    let object = row.object_owned()?;
                    row.purpose.validate_object(&object)?;
                }
                let (snapshot, lease) = retained.into_parts_v1();
                (snapshot.rows, lease)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (BTreeMap::new(), None),
            Err(e) => return Err(e.to_string()),
        };
        let temporary = parent.join(format!("{name}.pending"));
        if temporary.exists() {
            fs::remove_file(&temporary).map_err(|e| e.to_string())?;
        }
        Ok(Self { file: file.to_owned(), _file_lease: lock, _native_lease: lease, rows })
    }
    pub fn rows(&self) -> impl Iterator<Item = (&Hash64, &NativeCarrierQueueRowV4)> {
        self.rows.iter()
    }
    pub fn get(&self, id: &Hash64) -> Option<&NativeCarrierQueueRowV4> {
        self.rows.get(id)
    }
    pub fn provider_in_flight(&self, provider: PalwBondKeyV2) -> bool {
        self.rows.values().any(|r| matches!(&r.purpose,NativeCarrierPurposeV4::Answer {provider:p,..} if *p==provider))
    }
    pub fn pending_enrollment(&self, class: Hash64, executor: PalwBondKeyV2) -> Option<Hash64> {
        self.rows.iter().find_map(|(id, row)| {
            matches!(&row.purpose,NativeCarrierPurposeV4::Enrollment {class:c,executor:e,..} if *c==class&&*e==executor).then_some(*id)
        })
    }
    fn mutate(&mut self, id: Hash64, row: Option<NativeCarrierQueueRowV4>) -> Result<(), String> {
        if row.is_some() && !self.rows.contains_key(&id) && self.rows.len() >= MAX_ROWS {
            return Err("native carrier queue full before signing/publication".into());
        }
        let old_len = borsh::object_length(&(4u16, &self.rows)).map_err(|e| e.to_string())? as u64;
        let mut extra = 0;
        let mut encoded_extra = 0;
        let mut _row_lease = None;
        if let Some(row) = &row {
            let len = borsh::object_length(row).map_err(|e| e.to_string())? as u64;
            encoded_extra = len.checked_add(64).ok_or("native queue encoded row overflow")?;
            _row_lease = reserve_native_requests_v1(
                len.checked_add(68).and_then(|n| n.checked_mul(2)).ok_or("native queue row buffer overflow")?,
            )
            .map_err(|e| e.to_string())?;
            // Exact one-entry map frame lets the Core typed scanner count the
            // concrete BTree node envelope for this external owner type.
            let mut encoded = 1u32.to_le_bytes().to_vec();
            encoded.extend(id.as_byte_slice());
            borsh::to_writer(&mut encoded, row).map_err(|e| e.to_string())?;
            extra = preflight_native_value_v1::<BTreeMap<Hash64, NativeCarrierQueueRowV4>>(encoded.as_slice(), encoded.len() as u64)
                .map_err(|e| e.to_string())?
                .logical_requests_v1();
        }
        let max_encoded = old_len.checked_add(encoded_extra).ok_or("native queue encoded bound overflow")?;
        if max_encoded > MAX_BYTES {
            return Err("native carrier queue total byte capacity exhausted".into());
        }
        let base = self.rows.native_heap_v1("native carrier queue mutation").bytes().ok_or("native queue ownership unsupported")?;
        let requests = base
            .checked_mul(2)
            .and_then(|n| extra.checked_mul(2).and_then(|e| n.checked_add(e)))
            .and_then(|n| max_encoded.checked_mul(2).and_then(|e| n.checked_add(e)))
            .ok_or("native queue mutation memory overflow")?;
        let lease = reserve_native_requests_v1(requests).map_err(|e| e.to_string())?;
        let mut rows = self.rows.clone();
        if let Some(row) = row {
            rows.insert(id, row);
        } else {
            rows.remove(&id);
        }
        let encoded = borsh::to_vec(&(4u16, &rows)).map_err(|e| e.to_string())?;
        if encoded.len() as u64 > MAX_BYTES {
            return Err("native queue encoded capacity exceeded".into());
        }
        let parent = self.file.parent().unwrap();
        let name = self.file.file_name().unwrap().to_string_lossy();
        let tmp = parent.join(format!("{name}.pending"));
        let result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(|e| e.to_string())?;
            file.write_all(&encoded).map_err(|e| e.to_string())?;
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
    pub fn insert(&mut self, id: Hash64, purpose: NativeCarrierPurposeV4, object: &PalwConsensusObjectV2) -> Result<(), String> {
        if id != purpose.id() {
            return Err("native carrier key substitutes immutable purpose".into());
        }
        if let Some(old) = self.rows.get(&id) {
            let decoded = old.object_owned()?;
            if old.purpose != purpose || &*decoded != object {
                return Err("native carrier id replay substitutes object".into());
            }
            return Ok(());
        }
        let len = borsh::object_length(object).map_err(|e| e.to_string())? as u64;
        let _encoded =
            reserve_native_requests_v1(len.checked_mul(2).ok_or("native carrier object size overflow")?).map_err(|e| e.to_string())?;
        if len > MAX_BYTES {
            return Err("native carrier object exceeds declared queue cap".into());
        }
        purpose.validate_object(object)?;
        let object = borsh::to_vec(object).map_err(|e| e.to_string())?;
        self.mutate(id, Some(NativeCarrierQueueRowV4 { purpose, object, submitted_transaction: None }))
    }
    /// Bounded local proposal replacement: neither version authorizes GC or
    /// changes native obligations. Native acceptance rechecks exact tail and
    /// finality, even if the previously submitted transaction later arrives.
    pub fn replace_release_proposal(
        &mut self,
        id: Hash64,
        purpose: NativeCarrierPurposeV4,
        object: &PalwConsensusObjectV2,
    ) -> Result<(), String> {
        purpose.validate_object(object)?;
        if id != purpose.id() {
            return Err("release proposal key differs".into());
        }
        let NativeCarrierPurposeV4::Release { root, .. } = purpose else { return Err("proposal replacement is release-only".into()) };
        if let Some(old) = self.rows.get(&id) {
            if !matches!(old.purpose,NativeCarrierPurposeV4::Release{root:r,..} if r==root) {
                return Err("release proposal replaces foreign purpose/root".into());
            }
            let old_object = old.object_owned()?;
            if &*old_object == object {
                return Ok(());
            }
        }
        let len = borsh::object_length(object).map_err(|e| e.to_string())? as u64;
        if len > MAX_BYTES {
            return Err("release proposal exceeds queue cap".into());
        }
        let _copy = reserve_native_requests_v1(len.checked_mul(2).ok_or("release encoding overflow")?).map_err(|e| e.to_string())?;
        self.mutate(
            id,
            Some(NativeCarrierQueueRowV4 {
                purpose,
                object: borsh::to_vec(object).map_err(|e| e.to_string())?,
                submitted_transaction: None,
            }),
        )
    }
    pub fn record_submitted(&mut self, id: Hash64, tx: &Transaction) -> Result<(), String> {
        let row = self.rows.get(&id).ok_or("unknown native carrier")?;
        let version = kaspa_consensus_core::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2.to_le_bytes();
        if tx.payload.get(..2) != Some(version.as_slice()) || tx.payload.get(2..) != Some(row.object.as_slice()) {
            return Err("submitted carrier substitutes queued signed object".into());
        }
        let len = borsh::object_length(tx).map_err(|e| e.to_string())? as u64;
        let existing = row.native_heap_v1("native carrier submission").bytes().ok_or("native carrier row shape unsupported")?;
        let _copy = reserve_native_requests_v1(
            existing
                .checked_add(len.checked_mul(2).ok_or("native carrier transaction overflow")?)
                .ok_or("native carrier copy overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let mut next = row.clone();
        next.submitted_transaction = Some(borsh::to_vec(tx).map_err(|e| e.to_string())?);
        self.mutate(id, Some(next))
    }
    /// Caller has freshly verified positive native authority appropriate to the
    /// immutable purpose. Local absence or a successful send is insufficient.
    pub fn retire(&mut self, id: Hash64) -> Result<(), String> {
        if !self.rows.contains_key(&id) {
            return Ok(());
        }
        self.mutate(id, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_material_release_recovery_v2::{
            PalwMaterialRecoveryActionV2 as R, PalwMaterialRecoveryAnswerV4, material_recovery_object_v2,
        },
        palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as M,
        tx::TransactionOutpoint,
    };
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(h(n), 0))
    }
    fn release(root: Hash64, tail: Hash64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::MonetaryMaterialActionV1 {
            action: Box::new(M::Release { root, consent: h(2), tail_block: tail }),
            signer: bond(3),
            signature: vec![],
        }
    }
    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "misaka-native-carrier-v4-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir(&d).unwrap();
        d
    }
    /// Local persistence/proposal control only. No callback or empty queue here
    /// stands for native finality, physical payment, or an actual court verdict.
    #[test]
    fn release_reorg_proposals_share_one_slot_and_reopen_preserves_exact_object() {
        let dir = dir();
        let file = dir.join("queue");
        let mut queue = NativeCarrierQueueV4::open(&file).unwrap();
        let first = NativeCarrierPurposeV4::Release { root: h(1), consent: h(2), tail: h(4) };
        let id = first.id();
        queue.insert(id, first, &release(h(1), h(4))).unwrap();
        let second = NativeCarrierPurposeV4::Release { root: h(1), consent: h(2), tail: h(5) };
        assert!(queue.insert(id, second.clone(), &release(h(1), h(5))).is_err());
        queue.replace_release_proposal(id, second.clone(), &release(h(1), h(5))).unwrap();
        assert_eq!(queue.rows().count(), 1);
        drop(queue);
        let queue = NativeCarrierQueueV4::open(&file).unwrap();
        let row = queue.get(&id).unwrap();
        assert_eq!(row.purpose, second);
        assert_eq!(&*row.object_owned().unwrap(), &release(h(1), h(5)));
        drop(queue);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn answer_purpose_binds_signed_deadline_stream_sequence_and_event() {
        let action = R::AnswerAndCheckpoint {
            answer: Box::new(PalwMaterialRecoveryAnswerV4::Kernel {
                root: h(1),
                provider: bond(2),
                dependency: h(3),
                stage: 0,
                position: 1,
                bytes: vec![9],
            }),
            stream: h(4),
            previous: h(5),
            sequence: 7,
            challenge_deadline_daa: 100,
        };
        let event =
            Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/MATERIAL-ANSWER-EVENT/V4", &action.payload()));
        let object = material_recovery_object_v2(h(8), action, bond(2), |_, _, _| Some(vec![1])).unwrap();
        let purpose = NativeCarrierPurposeV4::Answer { provider: bond(2), stream: h(4), sequence: 7, event, deadline_daa: 100 };
        purpose.validate_object(&object).unwrap();
        let wrong = NativeCarrierPurposeV4::Answer { provider: bond(2), stream: h(4), sequence: 7, event, deadline_daa: 99 };
        assert!(wrong.validate_object(&object).is_err());
        let dir = dir();
        let mut q = NativeCarrierQueueV4::open(&dir.join("queue")).unwrap();
        assert!(q.insert(h(99), purpose.clone(), &object).is_err());
        q.insert(event, purpose, &object).unwrap();
        assert!(q.provider_in_flight(bond(2)));
        drop(q);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn nested_object_count_bomb_is_refused_even_without_native_gate() {
        let mut bytes = 4u16.to_le_bytes().to_vec();
        bytes.extend(1u32.to_le_bytes());
        bytes.extend([0u8; 64]);
        bytes.push(2);
        bytes.extend([0u8; 192]);
        bytes.extend(u32::MAX.to_le_bytes());
        assert!(preflight_native_value_v1::<Snapshot>(&bytes, bytes.len() as u64).is_err());
        assert!(decode_owned(&bytes, || borsh::from_slice::<Snapshot>(&bytes)).is_err());
    }
}
