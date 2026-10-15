//! Explicit private RFC08 checker material fetch (paid-v4 profile). A drawn checker node pages the complete TypedKernelV4
//! material of one claim from the named providers over the authenticated read-only transport and writes the assembled
//! bundle for the SDK consumer (`palw-class claim-v2 VERIFY-TYPED`), which re-authenticates every unit against the claim's
//! native commitments, reconstructs the material and runs the deterministic check.
//!
//! No shipped Args starts this. Fetching creates no consensus object, receipt, credit or Final; a provider serves only
//! under its own current native authority (reader seat, enrolled generation, context replay).
use super::kernel_paid_producer_v4::{hash, owned_dir, publish_new, regular};
use crate::palw_native_custody_v3::{PalwNativeCustodyContextV3, PalwNativeExchangeHandleV3};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateKernelPaidCheckSpecV4 {
    version: u16,
    root: String,
    preferred_consent: String,
    providers: Vec<String>,
    out_dir: PathBuf,
    max_material_bytes: u64,
}
impl PrivateKernelPaidCheckSpecV4 {
    pub fn read(path: &Path) -> Result<Self, String> {
        regular(path)?;
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let n = file.metadata().map_err(|e| e.to_string())?.len();
        if n > 64 * 1024 {
            return Err("kernel check spec exceeds its 64 KiB cap".into());
        }
        let mut bytes = Vec::with_capacity(n as usize + 1);
        file.take(n + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() as u64 != n {
            return Err("kernel check spec changed while reading".into());
        }
        let spec: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if spec.version != 4
            || spec.providers.is_empty()
            || spec.providers.len() > kaspa_consensus_core::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1
            || spec.max_material_bytes == 0
            || !spec.out_dir.is_absolute()
        {
            return Err("kernel check spec has invalid version, roster, cap or output path".into());
        }
        hash(&spec.root)?;
        hash(&spec.preferred_consent)?;
        for provider in &spec.providers {
            crate::palw_producer::parse_outpoint(provider)?;
        }
        Ok(spec)
    }
    pub fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedKernelPaidCheckV4, String> {
        if ctx.config.params.palw_material_release_recovery_v2.is_none() {
            return Err("kernel check requires the explicit paid-v4/recovery configuration".into());
        }
        let reader = PalwBondKeyV2(crate::palw_producer::parse_outpoint(
            ctx.producer_bond.as_deref().ok_or("kernel check reader bond absent (the drawn seat is this node's bond)")?,
        )?);
        let providers = self
            .providers
            .iter()
            .map(|p| crate::palw_producer::parse_outpoint(p).map(PalwBondKeyV2))
            .collect::<Result<Vec<_>, _>>()?;
        owned_dir(&self.out_dir)?;
        let lock_path = self.out_dir.join("checker.lock");
        if lock_path.exists() {
            regular(&lock_path)?;
        }
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("kernel check output is already owned: {e}"))?;
        Ok(PreparedKernelPaidCheckV4 {
            root: hash(&self.root)?,
            preferred: hash(&self.preferred_consent)?,
            providers,
            reader,
            out_dir: self.out_dir,
            max_material_bytes: self.max_material_bytes,
            manager: ctx.consensus_manager.clone(),
            tick: ctx.tick_service.clone(),
            _lock: lock,
        })
    }
}
pub struct PreparedKernelPaidCheckV4 {
    root: kaspa_consensus_core::Hash64,
    preferred: kaspa_consensus_core::Hash64,
    providers: Vec<PalwBondKeyV2>,
    reader: PalwBondKeyV2,
    out_dir: PathBuf,
    max_material_bytes: u64,
    manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    tick: Arc<kaspa_core::task::tick::TickService>,
    _lock: fs::File,
}
impl PreparedKernelPaidCheckV4 {
    pub fn start(self, exchange: PalwNativeExchangeHandleV3) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("private-kernel-paid-check-v4".into())
            .spawn(move || {
                if let Err(e) = runtime.block_on(self.run(exchange)) {
                    kaspa_core::warn!("private kernel paid checker stopped: {e}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn run(self, exchange: PalwNativeExchangeHandleV3) -> Result<(), String> {
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let api = self.manager.consensus().unguarded_session();
            let local_daa = api.get_virtual_daa_score();
            for provider in &self.providers {
                // The provider's native key comes from this node's own state; the transport never supplies it.
                let pubkey = match api.palw_bond_pubkey_v4(*provider) {
                    Ok(Some(key)) => key,
                    Ok(None) => continue,
                    Err(e) => {
                        kaspa_core::debug!("private kernel paid checker retries its native key read: {e}");
                        continue;
                    }
                };
                match exchange.0.read_kernel_material_v4(self.root, self.preferred, *provider, &pubkey, local_daa, self.max_material_bytes)
                {
                    Ok((bundle, consent, _leases)) => {
                        let bytes = borsh::to_vec(&bundle).map_err(|e| e.to_string())?;
                        let name = format!(
                            "{}-{}.typed-bundle-v4.borsh",
                            faster_hex::hex_string(&self.root.as_bytes()),
                            faster_hex::hex_string(&consent.as_bytes())
                        );
                        let path = self.out_dir.join(name);
                        if !path.exists() {
                            publish_new(&path, &bytes)?;
                        }
                        kaspa_core::info!(
                            "private kernel paid checker {:?} holds the provider bundle {} (unauthenticated until VERIFY-TYPED)",
                            self.reader.0,
                            path.display()
                        );
                        return Ok(());
                    }
                    Err(e) => kaspa_core::debug!("private kernel paid checker read from {:?} retries: {e}", provider.0),
                }
            }
        }
    }
}
