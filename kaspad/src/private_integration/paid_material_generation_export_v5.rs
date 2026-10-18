//! Read-only, one-generation own-node export. A file never authorizes a draw,
//! replacement or payment; the V5 continuation rechecks current native state.
use super::kernel_paid_producer_v4::{hash, owned_dir, publish_new, read_bounded, regular};
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use kaspa_consensus_core::{
    palw_native_resource_v1::{
        NativeDemandLeaseV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_state_v2::PalwConsensusObjectV2,
};
use misaka_palw_kernel::claim_paid_material_generation_v5::PaidClaimMaterialGenerationV5;
use std::{
    fs::{File, OpenOptions},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::Duration,
};

const GENERATION_BYTES: usize = 330;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivatePaidMaterialGenerationExportSpecV5 {
    version: u16,
    base_claim: String,
    original_object_path: PathBuf,
    output_path: PathBuf,
    expected_sequence: u64,
    expected_current_consent: String,
    max_input_bytes: u64,
    max_workspace_bytes: u64,
    #[serde(skip)]
    _owner: Option<NativeDemandLeaseV1>,
}
impl PrivatePaidMaterialGenerationExportSpecV5 {
    pub fn read(path: &Path) -> Result<Self, String> {
        let (raw, _raw) = read_bounded(path, 1 << 20)?;
        let owner = reserve_native_requests_v1(
            (raw.len() as u64)
                .checked_mul(8)
                .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
                .ok_or("generation export config overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let mut spec: Self = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if spec.version != 5
            || spec.expected_sequence == 0
            || spec.max_input_bytes == 0
            || spec.max_workspace_bytes == 0
            || spec.max_input_bytes > super::native_provider::PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3
        {
            return Err("generation export needs exact version, explicit replacement and finite input/workspace limits".into());
        }
        if hash(&spec.base_claim)? == Default::default() || hash(&spec.expected_current_consent)? == Default::default() {
            return Err("generation export root/consent cannot be zero".into());
        }
        regular(&spec.original_object_path)?;
        if !spec.output_path.is_absolute() || spec.output_path.file_name().is_none() {
            return Err("generation output must be an explicit absolute file".into());
        }
        spec._owner = owner;
        Ok(spec)
    }
    pub(super) fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedPaidMaterialGenerationExportV5, String> {
        if ctx.config.params.palw_claim_challenge_v2.is_none() || ctx.config.params.palw_material_release_recovery_v2.is_none() {
            return Err("generation export requires own paid/recovery capabilities".into());
        }
        owned_dir(self.output_path.parent().ok_or("generation output parent absent")?)?;
        if self.output_path.exists() {
            regular(&self.output_path)?;
        }
        let lock_path = self.output_path.with_extension("export-v5.lock");
        if lock_path.exists() {
            regular(&lock_path)?;
        }
        let lock =
            OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("generation export already owned: {e}"))?;
        let workspace = reserve_native_requests_v1(self.max_workspace_bytes).map_err(|e| e.to_string())?;
        let (raw, _raw) =
            read_bounded(&self.original_object_path, self.max_input_bytes.min(self.max_workspace_bytes.saturating_sub(1)))?;
        let shape = preflight_native_value_v1::<PalwConsensusObjectV2>(&raw, raw.len() as u64).map_err(|e| e.to_string())?;
        if shape.logical_requests_v1().checked_add(raw.len() as u64 + 65536).is_none_or(|n| n > self.max_workspace_bytes) {
            return Err("generation export original/decode/signature overlap exceeds explicit workspace".into());
        }
        let original =
            retain_native_value_decode_v1(&raw, || borsh::from_slice::<PalwConsensusObjectV2>(&raw)).map_err(|e| e.to_string())?;
        let PalwConsensusObjectV2::KernelRouteV1 { bytes, .. } = &*original else {
            return Err("generation export needs the original signed kernel carrier".into());
        };
        let body = bytes.get(1..).ok_or("generation original kernel version absent")?;
        let inner = preflight_native_value_v1::<misaka_palw_kernel::route::KernelRouteObjectV1>(body, body.len() as u64)
            .map_err(|e| e.to_string())?;
        if shape
            .logical_requests_v1()
            .checked_add(inner.logical_requests_v1())
            .and_then(|n| n.checked_add(raw.len() as u64 + 65536))
            .is_none_or(|n| n > self.max_workspace_bytes)
        {
            return Err("generation export exact nested kernel decode exceeds explicit workspace".into());
        }
        let expected_root = hash(&self.base_claim)?;
        let expected_consent = hash(&self.expected_current_consent)?;
        Ok(PreparedPaidMaterialGenerationExportV5 {
            spec: self,
            original,
            expected_root,
            expected_consent,
            manager: ctx.consensus_manager.clone(),
            tick: ctx.tick_service.clone(),
            _workspace: workspace,
            _lock: lock,
        })
    }
}
pub(super) struct PreparedPaidMaterialGenerationExportV5 {
    spec: PrivatePaidMaterialGenerationExportSpecV5,
    original: kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwConsensusObjectV2>,
    expected_root: kaspa_consensus_core::Hash64,
    expected_consent: kaspa_consensus_core::Hash64,
    manager: std::sync::Arc<kaspa_consensusmanager::ConsensusManager>,
    tick: std::sync::Arc<kaspa_core::task::tick::TickService>,
    _workspace: Option<NativeDemandLeaseV1>,
    _lock: File,
}
impl PreparedPaidMaterialGenerationExportV5 {
    pub(super) fn start(self) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("private-generation-export-v5".into())
            .spawn(move || {
                runtime.block_on(self.run());
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn run(self) {
        // Keep the whole owner and OS lock live for every poll and
        // publication, rather than capturing individual struct fields.
        let _held = (&self._workspace, &self._lock);
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                break;
            }
            let instance = self.manager.consensus();
            let api = instance.unguarded_session_blocking();
            let own = match api.palw_paid_material_generation_snapshot_v5(&self.original) {
                Ok(own) => own,
                Err(error) => {
                    kaspa_core::debug!("generation export own state unavailable: {error}");
                    continue;
                }
            };
            if own.generation.root != self.expected_root.as_bytes()
                || own.generation.sequence != self.spec.expected_sequence
                || own.generation.current_consent != self.expected_consent.as_bytes()
            {
                continue; // Third generations and rollback never overwrite the requested slot.
            }
            // This file is a one-slot immutable artifact, not a live
            // authority/status. Current native checks remain mandatory.
            match publish_exact(&self.spec.output_path, &own.generation) {
                Ok(()) => break,
                Err(error) => kaspa_core::warn!("generation export retained immutable output, retry pending: {error}"),
            }
        }
    }
}
fn publish_exact(path: &Path, generation: &PaidClaimMaterialGenerationV5) -> Result<(), String> {
    let mut bytes = [0u8; GENERATION_BYTES];
    let mut writer = Cursor::new(bytes.as_mut_slice());
    borsh::to_writer(&mut writer, generation).map_err(|e| e.to_string())?;
    if writer.position() != GENERATION_BYTES as u64 {
        return Err("generation fixed encoding extent differs".into());
    }
    if !path.exists() {
        return publish_new(path, &bytes);
    }
    regular(path)?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != GENERATION_BYTES as u64 {
        return Err("existing generation extent differs; original retained".into());
    }
    let mut existing = [0u8; GENERATION_BYTES];
    file.read_exact(&mut existing).map_err(|e| e.to_string())?;
    let mut trailing = [0u8; 1];
    if existing != bytes || file.read(&mut trailing).map_err(|e| e.to_string())? != 0 {
        return Err("existing generation content differs; original retained".into());
    }
    Ok(())
}

#[cfg(test)]
mod source_controls {
    use super::*;
    #[test]
    fn export_reopen_matches_exact_bytes_and_never_replaces_a_different_generation() {
        // Codec/publication control only; synthetic metadata is not own-node
        // admission, signature verification or a successful replacement.
        let directory = tempfile::tempdir().unwrap();
        let directory = directory.path().canonicalize().unwrap();
        let path = directory.join("generation.borsh");
        let generation = PaidClaimMaterialGenerationV5 {
            version: 5,
            enrollment: [1; 64],
            root: [2; 64],
            initial_consent: [3; 64],
            recovery: [4; 64],
            current_consent: [5; 64],
            sequence: 1,
        };
        publish_exact(&path, &generation).unwrap();
        publish_exact(&path, &generation).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(saved.len(), GENERATION_BYTES);
        let mut changed = generation;
        changed.sequence = 2;
        assert!(publish_exact(&path, &changed).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        std::fs::write(&path, &saved[..329]).unwrap();
        assert!(publish_exact(&path, &generation).is_err());
        assert_eq!(std::fs::read(&path).unwrap().len(), 329);
    }
}
