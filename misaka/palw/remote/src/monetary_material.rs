//! Durable typed provider for signed payment-relative material obligations.
//! A fixed old manifest TTL cannot delete these bytes. Every reopen rechecks
//! full codec coverage; GC requires the native finalized accepted-release API.
use kaspa_consensus_core::{
    Hash64,
    palw_material_capacity_v1::{PalwHeldMaterialBundleV1, PalwMaterialManifestV1, PalwMonetaryMaterialProviderStoreV1},
    palw_monetary_material_v1::{PalwMonetaryMaterialActionV1, PalwMonetaryMaterialEnrollmentV1},
    palw_state_v2::PalwBondKeyV2,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub struct DurableMonetaryMaterialProviderV1 {
    // The kernel releases this lease on crash; a PID-file lease would strand
    // capacity or allow concurrent snapshots to overwrite accepted material.
    _lease: File,
    file: PathBuf,
    max_contract_bytes: u64,
    total_capacity: u64,
    store: PalwMonetaryMaterialProviderStoreV1,
}
impl DurableMonetaryMaterialProviderV1 {
    /// Prospective consent is immutable. Ready binds the actual one-claim capture
    /// only after the whole typed bundle has been atomically published.
    pub fn prepare_native_ready_v3(&mut self,provider:PalwBondKeyV2,row:&kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1,ready:&kaspa_consensus_core::palw_attempt_material_v3::PalwNativeMaterialReadyV3,bundle:&PalwHeldMaterialBundleV1,authority:kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1)->Result<Hash64,String> {
        if row.terms.root!=ready.session || row.consent!=ready.consent || row.terms.native_session.is_none() || row.native_binding.is_some() || !ready.providers.contains(&provider) || bundle.claim!=ready.binding.claim || row.terms.max_material_bytes>self.max_contract_bytes {return Err("provider prospective session or exact ready differs".into())}
        let mut candidate=self.store.clone();
        candidate.admit_held(row.consent,&ready.binding.manifest,row.terms.max_material_bytes,self.total_capacity,std::slice::from_ref(bundle),|id|(id==bundle.claim).then_some(authority)).map_err(|e|e.to_string())?;
        self.publish(candidate)?;Ok(ready.id())
    }
    pub fn prepare_material(
        &mut self,
        provider: PalwBondKeyV2,
        enrollment: &PalwMonetaryMaterialEnrollmentV1,
        held: &[PalwHeldMaterialBundleV1],
        positions: &[kaspa_consensus_core::palw_material_capacity_v1::PalwKernelMaterialBundleV1],
        segmented: &[kaspa_consensus_core::palw_material_capacity_v1::PalwSegmentedMaterialBundleV1],
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
    ) -> Result<Hash64, String> {
        if !enrollment.offers.iter().any(|o| o.provider == provider) || enrollment.terms.max_material_bytes > self.max_contract_bytes {
            return Err("provider did not offer this complete material capacity".into());
        }
        let manifest = enrollment.terms.material_manifest.as_ref().ok_or("material enrollment lacks its exact signed manifest")?;
        if manifest.root != enrollment.terms.root {
            return Err("material manifest obligation was substituted".into());
        }
        let mut candidate = self.store.clone();
        candidate
            .admit_material(
                enrollment.id(),
                manifest,
                enrollment.terms.max_material_bytes,
                self.total_capacity,
                held,
                positions,
                segmented,
                ledger,
                authority,
            )
            .map_err(|e| e.to_string())?;
        self.publish(candidate)?;
        Ok(enrollment.id())
    }
    pub fn answer_material(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        demand: kaspa_consensus_core::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1,
    ) -> Result<PalwMonetaryMaterialActionV1, String> {
        self.store.answer_action(consent, root, dependency, provider, demand).map_err(|e| e.to_string())
    }
    pub fn open(
        file: &Path,
        max_contract_bytes: u64,
        total_capacity: u64,
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
    ) -> Result<Self, String> {
        if max_contract_bytes == 0 || total_capacity < max_contract_bytes {
            return Err("provider material capacity is invalid".into());
        }
        let parent = file.parent().ok_or("provider file has no parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = file.file_name().and_then(|name| name.to_str()).ok_or("provider file name is not UTF-8")?;
        let lease = acquire_provider_lease(&parent.join(format!("{name}.lock")))?;
        let store = match File::open(file) {
            Ok(mut source) => {
                let limit = total_capacity.checked_add(1 << 20).ok_or("provider persistence cap overflow")?;
                if source.metadata().map_err(|e| e.to_string())?.len() > limit {
                    return Err("provider snapshot exceeds its byte cap".into());
                }
                let mut bytes = Vec::new();
                source.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                PalwMonetaryMaterialProviderStoreV1::recover_verified(&bytes, max_contract_bytes, total_capacity, ledger, authority)
                    .map_err(|e| e.to_string())?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => PalwMonetaryMaterialProviderStoreV1::default(),
            Err(error) => return Err(error.to_string()),
        };
        // Pending snapshots were never signature-authorized: the signing API
        // returns only after atomic publication. With the kernel lease held,
        // these exact owned names cannot belong to a concurrent live writer.
        let prefix = format!("{name}.pending-");
        for entry in fs::read_dir(parent).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix(&prefix))
                .is_some_and(|suffix| suffix.parse::<u32>().is_ok())
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(Self { _lease: lease, file: file.to_path_buf(), max_contract_bytes, total_capacity, store })
    }
    fn publish(&mut self, candidate: PalwMonetaryMaterialProviderStoreV1) -> Result<(), String> {
        let encoded = candidate.encoded().map_err(|e| e.to_string())?;
        let parent = self.file.parent().ok_or("provider file has no parent directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = self.file.file_name().and_then(|name| name.to_str()).ok_or("provider filename is not UTF-8")?;
        let temporary = parent.join(format!("{name}.pending-{}", std::process::id()));
        // Never overwrite another writer's pending generation. Failure retains
        // the old memory/durable generation and does not authorize a signature.
        let mut output = OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| e.to_string())?;
        let result = (|| {
            output.write_all(&encoded)?;
            output.sync_all()?;
            fs::rename(&temporary, &self.file)?;
            File::open(parent)?.sync_all()?;
            Ok::<_, std::io::Error>(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(temporary);
            return Err(error.to_string());
        }
        self.store = candidate;
        Ok(())
    }
    /// Call before signing the enrollment. The complete canonical material and
    /// exact signed scope must be durable before a provider accepts liability.
    pub fn prepare_held(
        &mut self,
        provider: PalwBondKeyV2,
        enrollment: &PalwMonetaryMaterialEnrollmentV1,
        bundles: &[PalwHeldMaterialBundleV1],
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
    ) -> Result<Hash64, String> {
        if !enrollment.offers.iter().any(|offer| offer.provider == provider)
            || enrollment.terms.max_material_bytes > self.max_contract_bytes
        {
            return Err("provider did not offer this signed material capacity".into());
        }
        let manifest: &PalwMaterialManifestV1 =
            enrollment.terms.material_manifest.as_ref().ok_or("signed enrollment lacks a material manifest")?;
        let mut candidate = self.store.clone();
        candidate
            .admit_held(enrollment.id(), manifest, enrollment.terms.max_material_bytes, self.total_capacity, bundles, authority)
            .map_err(|e| e.to_string())?;
        self.publish(candidate)?;
        Ok(enrollment.id())
    }
    pub fn answer_held(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        missing: kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1,
    ) -> Result<PalwMonetaryMaterialActionV1, String> {
        self.store.held_answer_action(consent, root, dependency, provider, missing).map_err(|e| e.to_string())
    }
    pub fn prepare_replacement_held(
        &mut self,
        provider: PalwBondKeyV2,
        replacement: &kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialReplacementV1,
        bundles: &[PalwHeldMaterialBundleV1],
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
    ) -> Result<Hash64, String> {
        let e = &replacement.enrollment;
        if replacement.root != e.terms.root
            || !e.offers.iter().any(|o| o.provider == provider)
            || e.terms.max_material_bytes > self.max_contract_bytes
        {
            return Err("replacement substituted its provider/root/capacity".into());
        }
        let manifest = e.terms.material_manifest.as_ref().ok_or("replacement lacks its exact material manifest")?;
        let mut candidate = self.store.clone();
        candidate
            .admit_held(replacement.id(), manifest, e.terms.max_material_bytes, self.total_capacity, bundles, authority)
            .map_err(|e| e.to_string())?;
        self.publish(candidate)?;
        Ok(replacement.id())
    }
    pub fn gc_finalized(
        &mut self,
        root: Hash64,
        stored_consent: Hash64,
        release_consent: Hash64,
        tail: Hash64,
        release_block: kaspa_consensus_core::BlockHash,
        native_witness: impl FnMut(Hash64, Hash64, Hash64, kaspa_consensus_core::BlockHash) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let mut candidate = self.store.clone();
        if !candidate
            .release_if_finalized(root, stored_consent, release_consent, tail, release_block, native_witness)
            .map_err(|e| e.to_string())?
        {
            return Ok(false);
        }
        self.publish(candidate)?;
        Ok(true)
    }
    pub fn discard_expired_offer(
        &mut self,
        enrollment: &PalwMonetaryMaterialEnrollmentV1,
        native_expiry: impl FnMut(Hash64, u64) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let mut candidate = self.store.clone();
        if !candidate.discard_expired_offer(enrollment, native_expiry).map_err(|e| e.to_string())? {
            return Ok(false);
        }
        self.publish(candidate)?;
        Ok(true)
    }
}

#[cfg(unix)]
fn acquire_provider_lease(path: &Path) -> Result<File, String> {
    use std::os::fd::AsRawFd;
    let lease = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(|e| e.to_string())?;
    // SAFETY: flock uses the live File's descriptor; no pointers or aliasing.
    if unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!("provider snapshot is already leased or unavailable: {}", std::io::Error::last_os_error()));
    }
    Ok(lease)
}
#[cfg(not(unix))]
fn acquire_provider_lease(_path: &Path) -> Result<File, String> {
    Err("durable monetary material provider needs a kernel file lease on this platform".into())
}
