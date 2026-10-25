//! Bounded local intrinsic metadata, separately budgeted from material bytes.
//! Atomic publication precedes liability response. A row is identity data; only
//! a fresh positive native covenant can authorize replay/answers, and only the
//! native finalized GC witness can remove metadata after its material is gone.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_material_context_v4::{PalwKernelMaterialContextLimitsV4, PalwProspectiveKernelMaterialContextV4},
    palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1,
        NativeWireShapeV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
fn canonical_buffers_v4(length: u64, copies: u64) -> Result<u64, String> {
    length
        .max(1024)
        .checked_mul(2)
        .and_then(|n| n.checked_mul(copies))
        .ok_or("kernel metadata canonical buffer capacity overflow".into())
}
struct MetadataRamBudgetV4 {
    limit: u64,
    used: Mutex<u64>,
}
struct MetadataRamLeaseV4 {
    budget: Arc<MetadataRamBudgetV4>,
    bytes: u64,
}
impl MetadataRamBudgetV4 {
    fn reserve(self: &Arc<Self>, bytes: u64) -> Result<MetadataRamLeaseV4, String> {
        let mut used = self.used.lock().map_err(|_| "kernel metadata RAM budget poisoned")?;
        let next = used.checked_add(bytes).ok_or("kernel aggregate metadata RAM overflow")?;
        if next > self.limit {
            return Err("kernel metadata aggregate live owners exceed explicit RAM cap before allocation".into());
        }
        *used = next;
        Ok(MetadataRamLeaseV4 { budget: Arc::clone(self), bytes })
    }
}
impl Drop for MetadataRamLeaseV4 {
    fn drop(&mut self) {
        if let Ok(mut used) = self.budget.used.lock() {
            *used = used.saturating_sub(self.bytes);
        }
    }
}
/// Keeps both process-native and archive-local decode tickets through every
/// borrow of this returned row, including candidate replay and answer assembly.
pub struct KernelMaterialContextRecordOwnedV4 {
    owner: NativeRetainedV1<KernelMaterialContextRecordV4>,
    _ram: MetadataRamLeaseV4,
}
impl Deref for KernelMaterialContextRecordOwnedV4 {
    type Target = KernelMaterialContextRecordV4;
    fn deref(&self) -> &Self::Target {
        &self.owner
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct KernelMaterialContextRecordV4 {
    pub context: PalwProspectiveKernelMaterialContextV4,
    pub capsule: PalwMaterialRecoveryActionV2,
}
impl NativeWireScanV1 for KernelMaterialContextRecordV4 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        PalwProspectiveKernelMaterialContextV4::scan_native_wire_v1(r)?.merge(PalwMaterialRecoveryActionV2::scan_native_wire_v1(r)?)
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
struct Snapshot {
    version: u16,
    rows: BTreeMap<Hash64, Vec<u8>>,
}
impl NativeWireScanV1 for Snapshot {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        u16::scan_native_wire_v1(r)?.merge(BTreeMap::<Hash64, Vec<u8>>::scan_native_wire_v1(r)?)
    }
}
pub struct KernelMaterialContextArchiveV4 {
    file: PathBuf,
    _lock: File,
    limits: PalwKernelMaterialContextLimitsV4,
    rows: BTreeMap<Hash64, Vec<u8>>,
    _lease: Option<NativeDemandLeaseV1>,
    pending_expiry: Option<(Hash64, Hash64, u64)>,
    ram: Arc<MetadataRamBudgetV4>,
    _base_ram: MetadataRamLeaseV4,
    _base_native: Option<NativeDemandLeaseV1>,
    _map_ram: MetadataRamLeaseV4,
}
impl KernelMaterialContextArchiveV4 {
    pub fn open(file: &Path, limits: PalwKernelMaterialContextLimitsV4) -> Result<Self, String> {
        if limits.max_encoded_bytes <= 272 || limits.max_heap_bytes == 0 {
            return Err("kernel metadata needs explicit nonzero RAM/DB capacities".into());
        }
        let base =
            (std::mem::size_of::<Self>() + std::mem::size_of::<MetadataRamBudgetV4>() + 2 * std::mem::size_of::<usize>()) as u64;
        let base = base.checked_add(file.as_os_str().len() as u64).ok_or("kernel metadata owner size overflow")?;
        if base > limits.max_heap_bytes {
            return Err("kernel metadata base owner exceeds RAM cap before allocation".into());
        }
        let base_native = reserve_native_requests_v1(base).map_err(|e| e.to_string())?;
        let ram = Arc::new(MetadataRamBudgetV4 { limit: limits.max_heap_bytes, used: Mutex::new(base) });
        let base_ram = MetadataRamLeaseV4 { budget: Arc::clone(&ram), bytes: base };
        let paths = (file.as_os_str().len() as u64 + 64).checked_mul(8).ok_or("kernel metadata path workspace overflow")?;
        let _paths_ram = ram.reserve(paths)?;
        let _paths_native = reserve_native_requests_v1(paths).map_err(|e| e.to_string())?;
        let parent = file.parent().ok_or("kernel metadata needs an owned directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let lock = crate::monetary_material::acquire_provider_lease(&file.with_extension("kernel-context-lock-v4"))?;
        let (rows, lease, map_ram) = match File::open(file) {
            Ok(source) => {
                let len = source.metadata().map_err(|e| e.to_string())?.len();
                if len.checked_mul(2).and_then(|n| n.checked_add(272)).is_none_or(|n| n > limits.max_encoded_bytes) {
                    return Err("kernel metadata exceeds aggregate DB publication cap".into());
                }
                let raw_count = len.checked_add(1).and_then(|n| n.checked_mul(2)).ok_or("kernel metadata raw overflow")?;
                let _raw_ram = ram.reserve(raw_count)?;
                let _raw = reserve_native_requests_v1(raw_count).map_err(|e| e.to_string())?;
                let extent = usize::try_from(len.checked_add(1).ok_or("kernel metadata read extent overflow")?)
                    .map_err(|_| "kernel metadata read extent unsupported")?;
                let mut bytes = Vec::with_capacity(extent);
                source.take(len + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                if bytes.len() as u64 != len {
                    return Err("kernel metadata changed extent while opening".into());
                }
                let proof = preflight_native_value_v1::<Snapshot>(bytes.as_slice(), len).map_err(|e| e.to_string())?;
                if proof.logical_requests_v1() > limits.max_heap_bytes {
                    return Err("kernel metadata exceeds aggregate RAM cap".into());
                }
                let map_ram = ram.reserve(proof.logical_requests_v1())?;
                let owner =
                    retain_native_value_decode_v1(&bytes, || borsh::from_slice::<Snapshot>(&bytes)).map_err(|e| e.to_string())?;
                if owner.version != 4 || owner.rows.len() > 4096 {
                    return Err("kernel metadata version/count differs".into());
                }
                struct Compare<'a>(&'a [u8]);
                impl Write for Compare<'_> {
                    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                        if !self.0.starts_with(b) {
                            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "kernel metadata is not canonical"));
                        }
                        self.0 = &self.0[b.len()..];
                        Ok(b.len())
                    }
                    fn flush(&mut self) -> std::io::Result<()> {
                        Ok(())
                    }
                }
                let mut comparison = Compare(&bytes);
                borsh::to_writer(&mut comparison, &*owner).map_err(|e| e.to_string())?;
                if !comparison.0.is_empty() {
                    return Err("kernel metadata has noncanonical trailing bytes".into());
                }
                for encoded in owner.rows.values() {
                    let p = preflight_native_value_v1::<KernelMaterialContextRecordV4>(encoded.as_slice(), encoded.len() as u64)
                        .map_err(|e| e.to_string())?;
                    if p.logical_requests_v1() > limits.max_heap_bytes {
                        return Err("kernel metadata nested row exceeds explicit decode cap".into());
                    }
                }
                let (value, lease) = owner.into_parts_v1();
                (value.rows, lease, map_ram)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (BTreeMap::new(), None, ram.reserve(0)?),
            Err(e) => return Err(e.to_string()),
        };
        let pending = file.with_extension("kernel-context-pending-v4");
        if pending.exists() {
            fs::remove_file(pending).map_err(|e| e.to_string())?;
        }
        let expiry_file = file.with_extension("kernel-context-expiry-v4");
        let pending_expiry = match File::open(&expiry_file) {
            Ok(mut f) => {
                let mut bytes = [0u8; 136];
                f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
                let mut extra = [0u8; 1];
                if f.read(&mut extra).map_err(|e| e.to_string())? != 0 {
                    return Err("kernel expiry intent oversized".into());
                }
                Some(borsh::from_slice::<(Hash64, Hash64, u64)>(&bytes).map_err(|e| e.to_string())?)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        let temporary = expiry_file.with_extension("expiry-pending-v4");
        if temporary.exists() {
            fs::remove_file(temporary).map_err(|e| e.to_string())?;
        }
        Ok(Self {
            file: file.to_path_buf(),
            _lock: lock,
            limits,
            rows,
            _lease: lease,
            pending_expiry,
            ram,
            _base_ram: base_ram,
            _base_native: base_native,
            _map_ram: map_ram,
        })
    }
    pub fn pending_expiry(&self) -> Option<(Hash64, Hash64, u64)> {
        self.pending_expiry
    }
    pub fn next_consent(&self, after: Option<Hash64>) -> Option<Hash64> {
        after
            .and_then(|id| self.rows.range((std::ops::Bound::Excluded(id), std::ops::Bound::Unbounded)).next().map(|(id, _)| *id))
            .or_else(|| self.rows.keys().next().copied())
    }
    pub(crate) fn begin_expiry(&mut self, consent: Hash64, root: Hash64, until: u64) -> Result<(), String> {
        let _paths_ram = self
            .ram
            .reserve((self.file.as_os_str().len() as u64 + 64).checked_mul(8).ok_or("kernel metadata path workspace overflow")?)?;
        let _paths_native = reserve_native_requests_v1(_paths_ram.bytes).map_err(|e| e.to_string())?;
        let row = (consent, root, until);
        if let Some(old) = self.pending_expiry {
            return if old == row { Ok(()) } else { Err("kernel expiry intent already pending".into()) };
        }
        let file = self.file.with_extension("kernel-context-expiry-v4");
        let pending = file.with_extension("expiry-pending-v4");
        let mut bytes = [0u8; 136];
        borsh::to_writer(std::io::Cursor::new(&mut bytes[..]), &row).map_err(|e| e.to_string())?;
        let result = (|| {
            let mut f = OpenOptions::new().write(true).create_new(true).open(&pending).map_err(|e| e.to_string())?;
            f.write_all(&bytes).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&pending, &file).map_err(|e| e.to_string())?;
            File::open(file.parent().unwrap()).and_then(|f| f.sync_all()).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(pending);
        }
        result?;
        self.pending_expiry = Some(row);
        Ok(())
    }
    pub(crate) fn finish_expiry(&mut self, consent: Hash64) -> Result<(), String> {
        let _paths_ram = self
            .ram
            .reserve((self.file.as_os_str().len() as u64 + 64).checked_mul(8).ok_or("kernel metadata path workspace overflow")?)?;
        let _paths_native = reserve_native_requests_v1(_paths_ram.bytes).map_err(|e| e.to_string())?;
        if self.pending_expiry.is_none() {
            return Ok(());
        }
        if !self.pending_expiry.is_some_and(|(id, _, _)| id == consent) || self.rows.contains_key(&consent) {
            return Err("kernel expiry completion still has metadata or another intent".into());
        }
        let file = self.file.with_extension("kernel-context-expiry-v4");
        match fs::remove_file(&file) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        };
        File::open(file.parent().unwrap()).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        self.pending_expiry = None;
        Ok(())
    }
    pub fn limits(&self) -> PalwKernelMaterialContextLimitsV4 {
        self.limits
    }
    pub fn get(&self, consent: Hash64) -> Result<Option<KernelMaterialContextRecordOwnedV4>, String> {
        let Some(bytes) = self.rows.get(&consent) else { return Ok(None) };
        let p = preflight_native_value_v1::<KernelMaterialContextRecordV4>(bytes.as_slice(), bytes.len() as u64).map_err(|e| e.to_string())?;
        if p.logical_requests_v1() > self.limits.max_heap_bytes {
            return Err("kernel metadata row exceeds decode RAM cap".into());
        }
        let ram = self.ram.reserve(p.logical_requests_v1())?;
        let owner = retain_native_value_decode_v1(bytes, || borsh::from_slice::<KernelMaterialContextRecordV4>(bytes))
            .map_err(|e| e.to_string())?;
        Ok(Some(KernelMaterialContextRecordOwnedV4 { owner, _ram: ram }))
    }
    /// At most one immutable pending replacement for a root/previous/sequence.
    /// A later explicit deadline cannot silently extend an earlier signature.
    pub fn check_replacement_slot_v4(&self, capsule: &PalwMaterialRecoveryActionV2) -> Result<(), String> {
        let PalwMaterialRecoveryActionV2::ReplaceBeforeV4 { replacement, .. } = capsule else { return Ok(()) };
        let coverage = capsule.native_heap_v1("replacement slot borrowed capsule");
        let count = coverage
            .bytes()
            .ok_or_else(|| coverage.to_string())?
            .checked_add(std::mem::size_of::<PalwMaterialRecoveryActionV2>() as u64)
            .ok_or("replacement slot size overflow")?;
        let _ram = self.ram.reserve(count)?;
        let wire = borsh::object_length(capsule).map_err(|e| e.to_string())? as u64;
        let _native = reserve_native_requests_v1(
            count.checked_add(canonical_buffers_v4(wire, 2)?).ok_or("replacement slot workspace overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let _identity = self.ram.reserve(canonical_buffers_v4(wire, 2)?)?;
        let consent = capsule.bounded_replacement_consent_v4().ok_or("bounded replacement identity absent")?;
        for id in self.rows.keys() {
            if *id == consent {
                continue;
            }
            let old = self.get(*id)?.ok_or("replacement slot row vanished")?;
            if let PalwMaterialRecoveryActionV2::ReplaceBeforeV4 { replacement: prior, .. } = &old.capsule {
                if prior.root == replacement.root && prior.previous == replacement.previous && prior.sequence == replacement.sequence {
                    return Err("root/previous/sequence already has an immutable signed replacement deadline".into());
                }
            }
        }
        Ok(())
    }
    pub fn insert(
        &mut self,
        context: &PalwProspectiveKernelMaterialContextV4,
        capsule: &PalwMaterialRecoveryActionV2,
    ) -> Result<Hash64, String> {
        let borrowed = context.native_heap_v1("kernel metadata borrowed context").add::<KernelMaterialContextRecordV4>(
            capsule.native_heap_v1("kernel metadata borrowed capsule"),
            "kernel metadata borrowed input",
        );
        let borrowed = borrowed
            .bytes()
            .ok_or_else(|| borrowed.to_string())?
            .checked_add(std::mem::size_of::<KernelMaterialContextRecordV4>() as u64)
            .ok_or("kernel metadata borrowed roots overflow")?;
        let _borrowed_ram = self.ram.reserve(borrowed)?;
        let capsule_bytes = borsh::object_length(capsule).map_err(|e| e.to_string())? as u64;
        let _identity_ram = self.ram.reserve(canonical_buffers_v4(capsule_bytes, 2)?)?;
        let _identity = reserve_native_requests_v1(canonical_buffers_v4(capsule_bytes, 2)?).map_err(|e| e.to_string())?;
        let (monetary, consent, _) =
            kaspa_consensus_core::palw_kernel_material_enrollment_v4::kernel_material_capsule_identity_v4(capsule)?;
        if monetary.terms.root != context.intent.manifest.root
            || monetary.terms.material_manifest.as_ref() != Some(&context.intent.manifest)
            || monetary.terms.native_session.is_some()
        {
            return Err("kernel metadata capsule substitutes exact manifest".into());
        }
        self.check_replacement_slot_v4(capsule)?;
        let len = borsh::object_length(&(context, capsule)).map_err(|e| e.to_string())? as u64;
        if len > self.limits.max_encoded_bytes / 2 {
            return Err("kernel metadata row exceeds declared DB cap before publication".into());
        }
        let _raw_ram = self.ram.reserve(canonical_buffers_v4(len, 1)?)?;
        let _raw = reserve_native_requests_v1(canonical_buffers_v4(len, 1)?).map_err(|e| e.to_string())?;
        let bytes = borsh::to_vec(&(context, capsule)).map_err(|e| e.to_string())?;
        let proof = preflight_native_value_v1::<KernelMaterialContextRecordV4>(bytes.as_slice(), len).map_err(|e| e.to_string())?;
        if proof.logical_requests_v1() > self.limits.max_heap_bytes {
            return Err("kernel metadata nested decode exceeds aggregate RAM cap before publication".into());
        }
        if let Some(old) = self.rows.get(&consent) {
            // A later sponsor signature is not needed for metadata identity.
            // Keep one fixed capsule generation per consent; no retry growth.
            if old == &bytes {
                return Ok(consent);
            }
            let previous = self.get(consent)?.ok_or("existing kernel context disappeared")?;
            if previous.context.request != context.request || previous.capsule.payload() != capsule.payload() {
                return Err("kernel metadata consent replay substitutes context/capsule".into());
            }
            let (old, _, _) =
                kaspa_consensus_core::palw_kernel_material_enrollment_v4::kernel_material_capsule_identity_v4(&previous.capsule)?;
            let old_complete = old.offers.iter().all(|o| !o.signature.is_empty())
                && previous.capsule.provider_signatures().iter().all(|p| !p.signature.is_empty());
            let new_complete = monetary.offers.iter().all(|o| !o.signature.is_empty())
                && capsule.provider_signatures().iter().all(|p| !p.signature.is_empty());
            if old_complete || !new_complete {
                return Ok(consent);
            }
            // One bounded unsigned->signed capsule upgrade, never a new
            // consent/history row and never a downgrade on retry.
            self.publish(Some((consent, bytes)), None)?;
            return Ok(consent);
        }
        if self.rows.len() >= 4096 {
            return Err("kernel metadata contract capacity full before signing".into());
        }
        self.publish(Some((consent, bytes)), None)?;
        Ok(consent)
    }
    /// The caller has already applied the positive native finalized GC witness
    /// and atomically removed this exact material. Absence alone never calls it.
    pub(crate) fn remove_after_verified_gc(&mut self, consent: Hash64) -> Result<(), String> {
        if self.rows.contains_key(&consent) {
            self.publish(None, Some(consent))?;
        }
        Ok(())
    }
    fn publish(&mut self, insert: Option<(Hash64, Vec<u8>)>, remove: Option<Hash64>) -> Result<(), String> {
        let _paths_ram = self
            .ram
            .reserve((self.file.as_os_str().len() as u64 + 64).checked_mul(8).ok_or("kernel metadata path workspace overflow")?)?;
        let _paths_native = reserve_native_requests_v1(_paths_ram.bytes).map_err(|e| e.to_string())?;
        let component = "kernel metadata mutation";
        let old = self.rows.native_heap_v1(component);
        let extra = insert
            .as_ref()
            .map(|(_, b)| {
                b.len() as u64
                    + std::mem::size_of::<kaspa_consensus_core::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, Vec<u8>>>()
                        as u64
            })
            .unwrap_or(0);
        let requests =
            old.add::<Snapshot>(NativeCoverageV1::Bounded { bytes: extra + std::mem::size_of::<Snapshot>() as u64 }, component);
        let count = requests.bytes().ok_or_else(|| requests.to_string())?;
        if count > self.limits.max_heap_bytes {
            return Err("kernel metadata candidate exceeds aggregate RAM cap before clone".into());
        }
        let map_ram = self.ram.reserve(count)?;
        let lease = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
        let mut rows = self.rows.clone();
        if let Some(id) = remove {
            rows.remove(&id);
        }
        if let Some((id, b)) = insert {
            rows.insert(id, b);
        }
        let length = borsh::object_length(&(4u16, &rows)).map_err(|e| e.to_string())? as u64;
        if length.checked_mul(2).and_then(|n| n.checked_add(272)).is_none_or(|n| n > self.limits.max_encoded_bytes) {
            return Err("kernel metadata old/new aggregate DB cap exceeded".into());
        }
        let _raw_ram = self.ram.reserve(canonical_buffers_v4(length, 1)?)?;
        let _raw = reserve_native_requests_v1(canonical_buffers_v4(length, 1)?).map_err(|e| e.to_string())?;
        let encoded = borsh::to_vec(&(4u16, &rows)).map_err(|e| e.to_string())?;
        let pending = self.file.with_extension("kernel-context-pending-v4");
        let parent = self.file.parent().unwrap();
        let result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&pending).map_err(|e| e.to_string())?;
            file.write_all(&encoded).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&pending, &self.file).map_err(|e| e.to_string())?;
            File::open(parent).and_then(|f| f.sync_all()).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&pending);
        }
        result?;
        self.rows = rows;
        self._lease = lease;
        self._map_ram = map_ram;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            loop {
                let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!("misaka-kernel-context-v4-test-{}-{id}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("owned test directory: {e}"),
                }
            }
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn limits() -> PalwKernelMaterialContextLimitsV4 {
        PalwKernelMaterialContextLimitsV4 { max_encoded_bytes: 1 << 20, max_heap_bytes: 1 << 20 }
    }
    #[test]
    fn metadata_ram_budget_counts_all_live_owners_until_drop() {
        let budget = Arc::new(MetadataRamBudgetV4 { limit: 100, used: Mutex::new(0) });
        let persistent = budget.reserve(40).unwrap();
        let decoded = budget.reserve(40).unwrap();
        assert!(budget.reserve(21).is_err());
        assert_eq!(*budget.used.lock().unwrap(), 80);
        drop(decoded);
        let candidate = budget.reserve(60).unwrap();
        assert!(budget.reserve(1).is_err());
        drop(persistent);
        assert_eq!(*budget.used.lock().unwrap(), 60);
        drop(candidate);
        assert_eq!(*budget.used.lock().unwrap(), 0);
    }
    #[test]
    fn metadata_reopen_preflights_embedded_context_before_borsh() {
        let dir = TestDir::new();
        let file = dir.path().join("owned.context");
        let snapshot = Snapshot { version: 4, rows: BTreeMap::from([(Hash64::from_bytes([1; 64]), vec![0xff; 8])]) };
        fs::write(&file, borsh::to_vec(&snapshot).unwrap()).unwrap();
        assert!(KernelMaterialContextArchiveV4::open(&file, limits()).is_err());
    }
    #[test]
    fn fixed_expiry_intent_reopens_without_becoming_a_gc_authority() {
        let dir = TestDir::new();
        let file = dir.path().join("owned.context");
        let consent = Hash64::from_bytes([1; 64]);
        let root = Hash64::from_bytes([2; 64]);
        let mut archive = KernelMaterialContextArchiveV4::open(&file, limits()).unwrap();
        archive.begin_expiry(consent, root, 100).unwrap();
        assert!(archive.begin_expiry(consent, root, 99).is_err());
        drop(archive);
        let mut reopened = KernelMaterialContextArchiveV4::open(&file, limits()).unwrap();
        assert_eq!(reopened.pending_expiry(), Some((consent, root, 100)));
        // Finishing an already-empty local metadata intent changes no material,
        // native money or acceptance. The production caller rechecks native proof.
        reopened.finish_expiry(consent).unwrap();
        drop(reopened);
        assert_eq!(KernelMaterialContextArchiveV4::open(&file, limits()).unwrap().pending_expiry(), None);
    }
    #[test]
    fn metadata_never_invents_a_default_capacity() {
        let dir = TestDir::new();
        assert!(
            KernelMaterialContextArchiveV4::open(
                &dir.path().join("owned.context"),
                PalwKernelMaterialContextLimitsV4 { max_encoded_bytes: 0, max_heap_bytes: 0 }
            )
            .is_err()
        );
    }
}
