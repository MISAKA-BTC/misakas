//! Explicit private RFC08 bounded provider replacement (paid-v4 profile). The sponsor replays its OWN durable copy of the
//! signed work and intrinsic context (written by the kernel producer beside its outbox), obtains its own native
//! replacement grant, has every new provider persist the material and the new signed generation (241/242), queues the
//! atomic ReplaceBeforeV4 carrier, and then waits for its own native acceptance of the new generation.
//!
//! No shipped Args starts this. A queued carrier or a provider signature is not acceptance; the signed
//! `valid_until_daa` bounds the wait. Old generations, held bytes and earned cash stay native-owned.
use super::kernel_paid_producer_v4::{hash, owned_dir, publish_borsh, read_bounded, regular};
use crate::palw_native_custody_v3::{PalwNativeCustodyContextV3, PalwNativeExchangeHandleV3};
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
    palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementIntentV4,
    palw_native_resource_v1::{NativeDemandLeaseV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1},
    palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
    palw_state_v2::PalwBondKeyV2,
};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateKernelPaidReplaceSpecV4 {
    version: u16,
    root: String,
    providers: Vec<String>,
    valid_until_daa: u64,
    max_input_bytes: u64,
    #[serde(skip)]
    _owner: Option<NativeDemandLeaseV1>,
}
impl PrivateKernelPaidReplaceSpecV4 {
    pub fn read(path: &Path) -> Result<Self, String> {
        let (raw, _raw) = read_bounded(path, 64 * 1024)?;
        let parse = (raw.len() as u64)
            .checked_mul(8)
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
            .and_then(|n| n.checked_add(8 * std::mem::size_of::<String>() as u64))
            .ok_or("replacement spec metadata overflow")?;
        let owner = reserve_native_requests_v1(parse).map_err(|e| e.to_string())?;
        let mut spec: Self = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if spec.version != 4
            || !(2..=8).contains(&spec.providers.len())
            || spec.max_input_bytes == 0
            || spec.valid_until_daa == 0
        {
            return Err("kernel replacement spec has invalid version, roster, lifetime or input cap".into());
        }
        hash(&spec.root)?;
        let mut roster = spec
            .providers
            .iter()
            .map(|p| crate::palw_producer::parse_outpoint(p).map(PalwBondKeyV2))
            .collect::<Result<Vec<_>, _>>()?;
        let sorted = roster.clone();
        roster.sort();
        roster.dedup();
        if roster != sorted {
            return Err("kernel replacement roster must be sorted and distinct".into());
        }
        spec._owner = owner;
        Ok(spec)
    }
    pub fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedKernelPaidReplaceV4, String> {
        if ctx.config.params.palw_material_release_recovery_v2.is_none() {
            return Err("kernel replacement requires the explicit paid-v4/recovery configuration".into());
        }
        let sponsor = PalwBondKeyV2(crate::palw_producer::parse_outpoint(
            ctx.producer_bond.as_deref().ok_or("kernel replacement sponsor bond absent (the producer is the sponsor)")?,
        )?);
        let root = hash(&self.root)?;
        // The kernel producer's fixed, producer-owned outbox; no caller-chosen path.
        let dir = ctx
            .data_dir
            .join("kernel-paid-producer-v4")
            .join(faster_hex::hex_string(&kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&sponsor)))
            .join(faster_hex::hex_string(&root.as_bytes()));
        owned_dir(&dir)?;
        regular(&dir.join("request.borsh"))?;
        regular(&dir.join("context.borsh"))?;
        let lock_path = dir.join("replacement.lock");
        if lock_path.exists() {
            regular(&lock_path)?;
        }
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("kernel replacement is already owned: {e}"))?;
        let providers =
            self.providers.iter().map(|p| crate::palw_producer::parse_outpoint(p).map(PalwBondKeyV2)).collect::<Result<Vec<_>, _>>()?;
        Ok(PreparedKernelPaidReplaceV4 {
            root,
            providers,
            valid_until_daa: self.valid_until_daa,
            max_input_bytes: self.max_input_bytes,
            dir,
            manager: ctx.consensus_manager.clone(),
            tick: ctx.tick_service.clone(),
            _lock: lock,
            _spec: self,
        })
    }
}
pub struct PreparedKernelPaidReplaceV4 {
    root: Hash64,
    providers: Vec<PalwBondKeyV2>,
    valid_until_daa: u64,
    max_input_bytes: u64,
    dir: PathBuf,
    manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    tick: Arc<kaspa_core::task::tick::TickService>,
    _lock: fs::File,
    _spec: PrivateKernelPaidReplaceSpecV4,
}
impl PreparedKernelPaidReplaceV4 {
    pub fn start(self, exchange: PalwNativeExchangeHandleV3) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("private-kernel-paid-replace-v4".into())
            .spawn(move || {
                if let Err(e) = runtime.block_on(self.run(exchange)) {
                    kaspa_core::warn!("private kernel provider replacement stopped: {e}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn run(self, exchange: PalwNativeExchangeHandleV3) -> Result<(), String> {
        // The sponsor's own durable records, bounded and preflighted before Borsh allocates.
        let (raw, _raw_request) = read_bounded(&self.dir.join("request.borsh"), self.max_input_bytes)?;
        preflight_native_value_v1::<PalwProspectiveKernelMaterialRequestV4>(&raw[..], raw.len() as u64).map_err(|e| e.to_string())?;
        let request = retain_native_value_decode_v1(&raw, || borsh::from_slice::<PalwProspectiveKernelMaterialRequestV4>(&raw))
            .map_err(|e| e.to_string())?;
        let (raw, _raw_context) = read_bounded(&self.dir.join("context.borsh"), self.max_input_bytes)?;
        preflight_native_value_v1::<PalwProspectiveKernelMaterialContextV4>(&raw[..], raw.len() as u64)
            .map_err(|e| e.to_string())?;
        let context = retain_native_value_decode_v1(&raw, || borsh::from_slice::<PalwProspectiveKernelMaterialContextV4>(&raw))
            .map_err(|e| e.to_string())?;
        if context.intent.manifest.root != self.root || context.request != request.id() {
            return Err("sponsor's durable context/request belong to another root or work".into());
        }
        // One immutable intent per replacement: a restart reuses it (same proposal, same new generation consent).
        let intent_path = self.dir.join("replacement-intent.borsh");
        let mut intent: Option<PalwKernelMaterialReplacementIntentV4> = if intent_path.exists() {
            let (raw, _raw_intent) = read_bounded(&intent_path, 64 * 1024)?;
            Some(borsh::from_slice(&raw).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let mut queued = false;
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let api = self.manager.consensus().unguarded_session();
            let now = api.get_virtual_daa_score();
            let rows = match api.palw_monetary_material_covenants_v1() {
                Ok(rows) => rows,
                Err(e) => {
                    kaspa_core::debug!("private kernel replacement retries its covenant read: {e}");
                    continue;
                }
            };
            let Some(current) = rows.iter().find(|row| row.terms.root == self.root) else {
                return Err("the root has no current native covenant; nothing to replace".into());
            };
            if let Some(saved) = &intent {
                // Acceptance: the native row moved past the signed previous generation to the new sequence.
                if current.sequence >= saved.sequence {
                    kaspa_core::info!(
                        "private kernel provider replacement accepted natively for {} (sequence {})",
                        faster_hex::hex_string(&self.root.as_bytes()),
                        current.sequence
                    );
                    return Ok(());
                }
                if now > saved.valid_until_daa {
                    return Err("signed replacement lifetime expired before native acceptance; intent retained".into());
                }
            }
            if intent.is_none() {
                let fresh = PalwKernelMaterialReplacementIntentV4 {
                    version: 4,
                    root: self.root,
                    context_request: context.request,
                    previous: current.consent,
                    sequence: current.sequence.checked_add(1).ok_or("replacement sequence overflow")?,
                    providers: self.providers.clone(),
                    valid_until_daa: self.valid_until_daa,
                };
                publish_borsh(&intent_path, &fresh, 64 * 1024)?;
                intent = Some(fresh);
            }
            let saved = intent.as_ref().ok_or("replacement intent absent")?;
            if queued {
                continue;
            }
            let grant = match api.palw_kernel_material_replacement_v4(&context, &request.bundle, saved) {
                Ok(grant) => grant,
                Err(e) => {
                    kaspa_core::debug!("private kernel replacement retries its own native grant: {e}");
                    continue;
                }
            };
            match exchange.0.replace_kernel_material_v4(&context, &request.bundle, saved, &grant) {
                Ok(()) => queued = true,
                Err(e) => kaspa_core::debug!("private kernel replacement provider exchange retries: {e}"),
            }
        }
    }
}
