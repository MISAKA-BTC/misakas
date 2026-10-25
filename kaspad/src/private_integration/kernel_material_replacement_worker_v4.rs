//! Explicit private sender for a newly funded provider replacement. This worker
//! submits no native fee transaction itself: the receiving provider's already
//! configured payer drains its durable carrier queue. Data authentication and
//! local replay never stand in for randomly selected verifier approval.
use super::kernel_paid_producer_v4::{owned_dir, publish_borsh, read_bounded, regular};
use crate::palw_native_custody_v3::{PalwNativeCustodyContextV3, PalwNativeExchangeHandleV3};
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4 as Context,
    palw_kernel_material_enrollment_v4::kernel_material_capsule_identity_v4 as identity,
    palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementIntentV4 as Intent,
    palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2 as Recovery,
    palw_native_resource_v1::{
        NativeDemandLeaseV1, NativeRetainedV1, NativeWireScanV1, preflight_native_value_v1, reserve_native_requests_v1,
        retain_native_value_decode_v1,
    },
    palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4 as Bundle,
};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateKernelMaterialReplacementSpecV4 {
    version: u16,
    context_path: PathBuf,
    bundle_path: PathBuf,
    intent_path: PathBuf,
    max_input_bytes: u64,
    /// Aggregate persistent input objects plus raw/decode overlap only. The own
    /// native quote/candidate and publication use separate actual native leases.
    max_input_heap_bytes: u64,
    #[serde(skip)]
    _owner: Option<NativeDemandLeaseV1>,
}
impl PrivateKernelMaterialReplacementSpecV4 {
    pub fn read(path: &Path) -> Result<Self, String> {
        let (raw, _raw) = read_bounded(path, 1 << 20)?;
        let count = (raw.len() as u64)
            .checked_mul(8)
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
            .ok_or("replacement config size overflow")?;
        let owner = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
        let mut spec: Self = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if spec.version != 4
            || spec.max_input_bytes == 0
            || spec.max_input_bytes > super::native_provider::PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3
            || spec.max_input_heap_bytes == 0
        {
            return Err("replacement requires explicit finite input/heap limits and version 4".into());
        }
        for path in [&spec.context_path, &spec.bundle_path, &spec.intent_path] {
            regular(path)?;
        }
        spec._owner = owner;
        Ok(spec)
    }
    pub fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedKernelMaterialReplacementV4, String> {
        if ctx.config.params.palw_material_release_recovery_v2.is_none()
            || ctx.producer_key_path.is_none()
            || ctx.producer_bond.is_none()
        {
            return Err("explicit replacement requires recovery profile and own producer exchange key/bond".into());
        }
        let _prepare = reserve_native_requests_v1(
            (std::mem::size_of::<PreparedKernelMaterialReplacementV4>() as u64)
                .checked_add((ctx.data_dir.as_os_str().len() as u64 + 1024) * 8)
                .ok_or("replacement sender owner extent overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let mut inputs = 0u64;
        let context = decode_input::<Context>(&self.context_path, &self, &mut inputs)?;
        let bundle = decode_input::<Bundle>(&self.bundle_path, &self, &mut inputs)?;
        let intent = decode_input::<Intent>(&self.intent_path, &self, &mut inputs)?;
        if intent.version != 4
            || context.version != 4
            || intent.root != context.intent.manifest.root
            || intent.context_request != context.request
            || !(2..=8).contains(&intent.providers.len())
            || intent.providers.windows(2).any(|p| p[0] >= p[1])
        {
            return Err("replacement input identities/roster differ".into());
        }
        let sponsor = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(crate::palw_producer::parse_outpoint(
            ctx.producer_bond.as_ref().unwrap(),
        )?);
        if sponsor != context.intent.producer {
            return Err("replacement configured sponsor differs from original work producer".into());
        }
        let dir = ctx.data_dir.join("kernel-material-replacement-v4");
        owned_dir(&dir)?;
        let lock = dir.join("sender.lock");
        if lock.exists() {
            regular(&lock)?;
        }
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("replacement sender already owned: {e}"))?;
        let id = Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/KERNEL-REPLACEMENT-SENDER/V4", &*intent));
        let outbox = dir.join(faster_hex::hex_string(id.as_byte_slice()));
        owned_dir(&outbox)?;
        Ok(PreparedKernelMaterialReplacementV4 {
            spec: self,
            context,
            bundle,
            intent,
            input_requests: inputs,
            manager: ctx.consensus_manager.clone(),
            tick: ctx.tick_service.clone(),
            outbox,
            _lock: lock,
            _prepare,
        })
    }
}
fn decode_input<T: NativeWireScanV1 + borsh::BorshDeserialize>(
    path: &Path,
    spec: &PrivateKernelMaterialReplacementSpecV4,
    inputs: &mut u64,
) -> Result<NativeRetainedV1<T>, String> {
    // Enforce the input budget before the raw file allocation as well as the
    // later typed decode. A changed file cannot consume the decode headroom.
    let raw_cap = spec
        .max_input_heap_bytes
        .checked_sub(*inputs)
        .and_then(|n| n.checked_sub(1))
        .ok_or("replacement has no raw input headroom")?
        .min(spec.max_input_bytes);
    let (raw, _raw) = read_bounded(path, raw_cap)?;
    let shape = preflight_native_value_v1::<T>(raw.as_slice(), raw.len() as u64).map_err(|e| e.to_string())?;
    let next = inputs.checked_add(shape.logical_requests_v1()).ok_or("replacement aggregate input heap overflow")?;
    let overlap = next.checked_add(raw.len() as u64 + 1).ok_or("replacement raw/decode overlap overflow")?;
    if overlap > spec.max_input_heap_bytes {
        return Err("replacement input owners/raw/decode overlap exceed explicit input heap cap".into());
    }
    let owner = retain_native_value_decode_v1(&raw, || borsh::from_slice::<T>(&raw)).map_err(|e| e.to_string())?;
    *inputs = next;
    Ok(owner)
}
pub struct PreparedKernelMaterialReplacementV4 {
    spec: PrivateKernelMaterialReplacementSpecV4,
    context: NativeRetainedV1<Context>,
    bundle: NativeRetainedV1<Bundle>,
    intent: NativeRetainedV1<Intent>,
    input_requests: u64,
    manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    tick: Arc<kaspa_core::task::tick::TickService>,
    outbox: PathBuf,
    _lock: fs::File,
    _prepare: Option<NativeDemandLeaseV1>,
}
impl PreparedKernelMaterialReplacementV4 {
    pub fn start(self, exchange: PalwNativeExchangeHandleV3) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("private-material-replacement-v4".into())
            .spawn(move || {
                if let Err(e) = runtime.block_on(self.run(exchange)) {
                    kaspa_core::warn!("private material replacement stopped: {e}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn run(self, exchange: PalwNativeExchangeHandleV3) -> Result<(), String> {
        let proposal_path = self.outbox.join("proposal.borsh");
        let mut proposal = if proposal_path.exists() {
            let mut count = self.input_requests;
            Some(decode_input::<Recovery>(&proposal_path, &self.spec, &mut count)?)
        } else {
            None
        };
        if let Some(saved) = &proposal {
            self.validate_proposal(saved)?;
        }
        let mut warned = false;
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let api = self.manager.consensus().unguarded_session_blocking();
            if let Some(saved) = &proposal {
                let (e, consent, _) = identity(saved)?;
                let Recovery::ReplaceBeforeV4 { recovery, .. } = &**saved else { unreachable!() };
                if api.palw_material_recovery_enrollment_finalized_v4(e.terms.root, consent, *recovery)? {
                    self.record_status("native-finalized.borsh", &(e.terms.root, consent, *recovery))?;
                    return Ok(());
                }
                if api.palw_kernel_material_capsule_unused_finalized_v4(saved)? {
                    // This is current native impossibility plus no live exact
                    // generation; it is not a claim that it never existed.
                    self.record_status("native-deadline-no-current-generation.borsh", &(e.terms.root, consent, *recovery))?;
                    return Ok(());
                }
            }
            if api.get_virtual_daa_score() > self.intent.valid_until_daa {
                if proposal.is_none() {
                    return Err("replacement deadline passed before a durable proposal/signature request".into());
                }
                continue;
            }
            let grant = match api.palw_kernel_material_replacement_v4(&self.context, &self.bundle, &self.intent) {
                Ok(grant) => grant,
                Err(reason) => {
                    if !warned {
                        kaspa_core::warn!("private replacement preparation pending; no acceptance inferred: {reason}");
                        warned = true;
                    }
                    continue;
                }
            };
            self.validate_proposal(&grant.proposal)?;
            if let Some(saved) = &proposal {
                if saved.payload() != grant.proposal.payload() {
                    return Err("replacement retry changes its durable signed subject".into());
                }
            } else {
                // Immutable plan before any provider can expose a signature.
                publish_borsh(&proposal_path, &*grant.proposal, self.spec.max_input_bytes)?;
                let mut count = self.input_requests;
                proposal = Some(decode_input::<Recovery>(&proposal_path, &self.spec, &mut count)?);
            }
            if let Err(e) = exchange.0.replace_kernel_material_v4(&self.context, &self.bundle, &self.intent, &grant) {
                if !warned {
                    kaspa_core::warn!("private material replacement pending; native acceptance still unconfirmed: {e}");
                    warned = true;
                }
            }
            // No queued/submitted/local marker is accepted as a native result.
            // Reads are repeated against current own state across reopen/reorg.
        }
    }
    fn validate_proposal(&self, proposal: &Recovery) -> Result<(), String> {
        let (e, _, until) = identity(proposal)?;
        let Recovery::ReplaceBeforeV4 { replacement, providers, .. } = proposal else {
            return Err("replacement WAL is not a bounded native generation".into());
        };
        if until != self.intent.valid_until_daa
            || replacement.root != self.intent.root
            || replacement.previous != self.intent.previous
            || replacement.sequence != self.intent.sequence
            || e.terms.sponsor != self.context.intent.producer
            || e.terms.material_manifest.as_ref() != Some(&self.context.intent.manifest)
            || e.offers.iter().map(|o| &o.provider).ne(self.intent.providers.iter())
            || providers.iter().map(|p| &p.provider).ne(self.intent.providers.iter())
        {
            return Err("replacement WAL substitutes input work/generation/roster/deadline".into());
        }
        Ok(())
    }
    fn record_status<T: borsh::BorshSerialize>(&self, name: &str, value: &T) -> Result<(), String> {
        let path = self.outbox.join(name);
        if path.exists() {
            regular(&path)?;
            return Ok(());
        }
        publish_borsh(&path, value, self.spec.max_input_bytes)
    }
}
