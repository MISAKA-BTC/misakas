//! **RFC-0008 PAV3 loopback provider for the private paid-v3 profile** (lead default B, 2026-10-15; unverified source,
//! never compiled or run here). Feature-gated with the rest of `private_integration`: ordinary kaspad Args cannot select it.
//!
//! A provider service owns, per configured fixture provider bond, a [`DurableMonetaryMaterialProviderV1`] store under the
//! node's private data dir, and signs a producer's Ready only after it
//!
//! 1. obtained the material authority from ITS OWN node's Consensus API
//!    (`palw_native_material_provider_authority_v3`, which re-runs the canonical session selection at the checkpoint),
//! 2. re-derived the job prompt from the anchor and rebuilt the complete typed bundle from the capture with its own backend
//!    (`palw_native_custody_v3::palw_native_bundle_v3`, the one function the producer uses too), and found the producer's
//!    manifest identical,
//! 3. checked that the chain's registered key for its roster bond is the key it holds, and that the executor is not one of
//!    its own operators,
//! 4. durably published the bundle (`misaka_palw_remote::native_ready_v3::provider_sign_native_ready_v3`).
//!
//! Keys are the private fixture's seat seeds (the files `export_private_complete_bootstrap_v1` writes, loaded with
//! `load_validator_seed` like every private seat key) named with their bond outpoints; each must be the salt's own card for
//! that bond, at least two distinct card operators are required, and none may be the local producer's key, bond or operator.
//! Transport: in-process (this node's own producer), or one framed request per connection over TCP carrying the
//! integrity-authenticated provider envelope (`misaka_palw_remote::provider_transport_v1`, 2026-10-15 round 3): the request is
//! signed by the executor's registered bond key, every response by each answering provider bond's registered key, with the
//! network domain, a nonce, the wall clock and the DAA score bound and a replay guard on the provider. The loopback socket is
//! one instance of it; `--private-probe-native-network=remote` admits separate hosts (still feature-gated, private only). It
//! is TLS-free by design: the material is public, so nothing is encrypted — integrity and authentication are the guarantees.
//!
//! The service also runs the provider GC janitor (`misaka_palw_remote::monetary_material_gc`) over its own Consensus API: a
//! retained bundle is deleted only under `palw_monetary_material_release_finalized_v1` (accepted native release AND
//! selected-chain finality of the release block), with a write-ahead intent marker; every failed read retains.
use crate::palw_native_custody_v3::{
    NativeReadyExchangeV3, PALW_NATIVE_READY_EXCHANGE_VERSION_V3, PalwNativeCustodyContextV3, PalwNativeCustodyHookV3,
    PalwNativeExchangeHandleV3, PalwNativeReadyRequestV3, PalwNativeReadyResponseV3, palw_native_bundle_v3, palw_native_prompt_v3,
};
use kaspa_consensus_core::{
    palw_material_capacity_v1::PalwMaterialNativeAuthorityV1,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_state_v2::{PalwBondKeyV2, palw_operator_id_v2},
};
use kaspa_hashes::Hash64;
use misaka_palw_remote::{
    monetary_material::DurableMonetaryMaterialProviderV1,
    monetary_material_gc::{MonetaryMaterialGcJanitorV1, MonetaryMaterialGcOracleV1},
    provider_transport_v1::{
        PALW_PROVIDER_TRANSPORT_REPLAY_CAPACITY_V1, ProviderTransportPolicyV1, ProviderTransportPurposeV1,
        ProviderTransportReplayGuardV1, ProviderTransportRequestV1, ProviderTransportResponseV1, ProviderTransportSignerV1,
        provider_transport_request_v1, provider_transport_response_v1, provider_transport_verify_request_v1,
        provider_transport_verify_response_v1,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

/// One framed request or response; a private fixture capture is small, and an unbounded length is refused.
pub const PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3: u64 = 1 << 30;
/// How long one exchange may take end to end (two bundle rebuilds and fsyncs on a private fixture class).
pub const PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3: Duration = Duration::from_secs(600);
/// How long the provider waits on any single read from a peer (a stalled peer is dropped, not waited on for the full exchange).
pub const PRIVATE_NATIVE_EXCHANGE_IDLE_TIMEOUT_V3: Duration = Duration::from_secs(60);
/// Connections a provider service handles at once; more are closed unanswered.
pub const PRIVATE_NATIVE_EXCHANGE_MAX_CONNECTIONS_V3: usize = 4;
/// How often the provider GC janitor runs.
pub const PRIVATE_NATIVE_PROVIDER_GC_PERIOD_V3: Duration = Duration::from_secs(60);

/// One configured provider bond and the fixture seed that signs for it.
pub struct PrivateNativeProviderKeyV3 {
    pub bond: PalwBondKeyV2,
    pub seed_path: PathBuf,
    key: kaspa_pq_validator_core::ValidatorKey,
}

/// Which addresses the provider socket options accept (`--private-probe-native-network`, default `loopback`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateNativeNetworkV3 {
    /// 127.0.0.1 / ::1 only, as the drill runs it.
    Loopback,
    /// Any explicit address with a port: separate hosts. Every frame is still the authenticated envelope; nothing is
    /// encrypted (the material is public).
    Remote,
}

/// **The private native-custody wiring**, parsed from the private node's `--private-probe-native-*` options only.
pub struct PrivateNativeCustodySpecV3 {
    /// Provider bonds this node serves (empty: none).
    pub providers: Vec<PrivateNativeProviderKeyV3>,
    /// Serve them to other private nodes on this socket. Without it they serve this node's own producer in-process.
    pub listen: Option<SocketAddr>,
    /// This node's producer asks the provider service at this socket (authenticated with its own producer key).
    pub exchange: Option<SocketAddr>,
    pub network: PrivateNativeNetworkV3,
}

fn socket(name: &str, value: &str, network: PrivateNativeNetworkV3, dial: bool) -> Result<SocketAddr, String> {
    let addr: SocketAddr = value.parse().map_err(|e| format!("{name}={value}: {e}"))?;
    if addr.port() == 0 {
        return Err(format!("{name} must name an explicit port"));
    }
    match network {
        PrivateNativeNetworkV3::Loopback if !addr.ip().is_loopback() => Err(format!(
            "{name} must be an explicit loopback address with a port (separate hosts need --private-probe-native-network=remote)"
        )),
        PrivateNativeNetworkV3::Remote if dial && addr.ip().is_unspecified() => {
            Err(format!("{name} cannot dial an unspecified address"))
        }
        _ => Ok(addr),
    }
}

impl PrivateNativeCustodySpecV3 {
    /// `keys`: `SEED_FILE@TXID:INDEX[,SEED_FILE@TXID:INDEX...]`; `network`: `loopback` (default) or `remote`. `None` when no
    /// option was given.
    pub fn parse(
        keys: Option<&str>,
        listen: Option<&str>,
        exchange: Option<&str>,
        network: Option<&str>,
    ) -> Result<Option<Self>, String> {
        if keys.is_none() && listen.is_none() && exchange.is_none() {
            return match network {
                None => Ok(None),
                Some(_) => Err("--private-probe-native-network needs a provider socket option".into()),
            };
        }
        let network = match network {
            None | Some("loopback") => PrivateNativeNetworkV3::Loopback,
            Some("remote") => PrivateNativeNetworkV3::Remote,
            Some(other) => return Err(format!("--private-probe-native-network={other}: expected loopback or remote")),
        };
        if network == PrivateNativeNetworkV3::Remote && listen.is_none() && exchange.is_none() {
            return Err("--private-probe-native-network=remote needs a provider listener or exchange".into());
        }
        let mut providers = Vec::new();
        if let Some(keys) = keys {
            for entry in keys.split(',') {
                let (file, bond) =
                    entry.rsplit_once('@').ok_or_else(|| format!("provider key '{entry}' is not SEED_FILE@TXID:INDEX"))?;
                let path = PathBuf::from(file);
                if !path.is_absolute() {
                    return Err(format!("provider seed file {file} must be an absolute fixture path"));
                }
                let seed = kaspa_pq_validator_core::load_validator_seed(file)?;
                let bond = PalwBondKeyV2(crate::palw_producer::parse_outpoint(bond)?);
                providers.push(PrivateNativeProviderKeyV3 {
                    bond,
                    seed_path: path,
                    key: kaspa_pq_validator_core::ValidatorKey::from_seed(seed),
                });
            }
            let bonds: BTreeSet<_> = providers.iter().map(|p| p.bond).collect();
            let keys: BTreeSet<_> = providers.iter().map(|p| p.key.public_key().to_vec()).collect();
            if bonds.len() != providers.len() || keys.len() != providers.len() {
                return Err("private native providers need distinct bonds and distinct keys".into());
            }
            if providers.len() < 2 || providers.len() > kaspa_consensus_core::palw_attempt_material_v3::PALW_MATERIAL_MAX_PROVIDERS_V3
            {
                return Err("a private native provider service holds 2..=8 roster bonds".into());
            }
        }
        let listen = listen.map(|v| socket("--private-probe-native-provider-listen", v, network, false)).transpose()?;
        let exchange = exchange.map(|v| socket("--private-probe-native-exchange", v, network, true)).transpose()?;
        if listen.is_some() && providers.is_empty() {
            return Err("--private-probe-native-provider-listen needs --private-probe-native-provider-keys".into());
        }
        if exchange.is_some() && !providers.is_empty() && listen.is_none() {
            return Err("in-process providers and a socket exchange are exclusive for one producer".into());
        }
        if exchange.is_some() && exchange == listen {
            return Err("a node does not dial its own provider socket; omit the listener to serve it in-process".into());
        }
        Ok(Some(Self { providers, listen, exchange, network }))
    }

    /// **The fixture check**: each provider seed is the salt's card key for that exact bond; at least two distinct card
    /// operators; and, on a node that produces, none is its producer's bond, key or operator (an observer that names a seat's
    /// producer key without `--palw-produce` may host other seats). Runs before any service or DB opens. A node reopened
    /// without `--palw-produce` keeps an unused exchange: nothing calls it.
    pub fn check_fixture(&self, config: &kaspa_consensus_core::config::Config, args: &crate::args::Args) -> Result<(), String> {
        let salt = config.palw_drill_genesis_salt.as_ref().ok_or("private native providers need the salted fixture")?;
        let cards: BTreeMap<PalwBondKeyV2, kaspa_consensus_core::config::drill::PalwDrillCardV1> =
            kaspa_consensus_core::config::drill::palw_t12_drill_cards_v1(salt)
                .into_iter()
                .map(|card| {
                    (
                        PalwBondKeyV2(kaspa_consensus_core::config::drill::palw_t12_drill_premine_outpoint_v1(
                            salt,
                            card.premine_index,
                        )),
                        card,
                    )
                })
                .collect();
        let mut operators = BTreeSet::new();
        for provider in &self.providers {
            let card = cards.get(&provider.bond).ok_or("a private native provider bond is not a fixture seat")?;
            if card.bond.pubkey.as_slice() != provider.key.public_key() {
                return Err(format!("provider seed {} is not the fixture key of its bond", provider.seed_path.display()));
            }
            operators.insert(palw_operator_id_v2(&card.operator.pubkey));
        }
        if !self.providers.is_empty() && operators.len() < 2 {
            return Err("private native providers need at least two distinct operators".into());
        }
        if let Some(bond) = args.palw_producer_bond.as_ref().filter(|_| args.palw_produce) {
            let bond = PalwBondKeyV2(crate::palw_producer::parse_outpoint(bond)?);
            if self.providers.iter().any(|p| p.bond == bond) {
                return Err("a private native provider is this node's own producer bond".into());
            }
            if let Some(card) = cards.get(&bond)
                && operators.contains(&palw_operator_id_v2(&card.operator.pubkey))
            {
                return Err("a private native provider shares the executor's operator; self-custody is refused".into());
            }
        }
        if let Some(path) = args.palw_producer_key.as_ref().filter(|_| args.palw_produce) {
            let producer = kaspa_pq_validator_core::ValidatorKey::from_seed(kaspa_pq_validator_core::load_validator_seed(path)?);
            if self.providers.iter().any(|p| p.key.public_key() == producer.public_key()) {
                return Err("a private native provider key is this node's own producer key".into());
            }
        }
        Ok(())
    }

    /// The daemon hook: open the provider service (its authenticated listener and its GC janitor), and return the exchange for
    /// this node's producer.
    pub fn into_hook(self) -> PalwNativeCustodyHookV3 {
        self.into_hook_internal_v3(None)
    }
    pub fn into_hook_with_service_v4(
        self,
        on_open: Box<dyn FnOnce(Arc<LoopbackNativeProviderV3>, &PalwNativeCustodyContextV3) -> Result<(), String> + Send>,
    ) -> PalwNativeCustodyHookV3 {
        self.into_hook_internal_v3(Some(on_open))
    }
    fn into_hook_internal_v3(
        self,
        on_open: Option<Box<dyn FnOnce(Arc<LoopbackNativeProviderV3>, &PalwNativeCustodyContextV3) -> Result<(), String> + Send>>,
    ) -> PalwNativeCustodyHookV3 {
        Box::new(move |ctx: PalwNativeCustodyContextV3| -> Result<Option<PalwNativeExchangeHandleV3>, String> {
            let Self { providers, listen, exchange, network } = self;
            let service = if providers.is_empty() { None } else { Some(Arc::new(LoopbackNativeProviderV3::open(&ctx, providers)?)) };
            if let Some(on_open) = on_open {
                on_open(Arc::clone(service.as_ref().ok_or("private v4 ACK adapter requires a provider service")?), &ctx)?;
            }
            if let Some(service) = &service {
                let service = Arc::clone(service);
                std::thread::Builder::new()
                    .name("palw-native-provider-gc-v3".into())
                    .spawn(move || {
                        loop {
                            std::thread::sleep(PRIVATE_NATIVE_PROVIDER_GC_PERIOD_V3);
                            service.gc_tick();
                        }
                    })
                    .map_err(|e| e.to_string())?;
            }
            if let (Some(service), Some(listen)) = (&service, listen) {
                let listener = TcpListener::bind(listen).map_err(|e| format!("native provider listener {listen}: {e}"))?;
                let service = Arc::clone(service);
                std::thread::Builder::new()
                    .name("palw-native-provider-v3".into())
                    .spawn(move || serve_authenticated_v3(listener, service, network))
                    .map_err(|e| e.to_string())?;
                kaspa_core::warn!(
                    "RFC12 PRIVATE: PAV3 native provider service listening on {listen} ({network:?}; authenticated, unencrypted; \
                     unapproved mechanics)"
                );
            }
            Ok(match (exchange, service, listen) {
                (Some(addr), _, _) => {
                    let key =
                        ctx.producer_key_path.as_deref().ok_or("an authenticated provider exchange needs --palw-producer-key")?;
                    let bond = ctx.producer_bond.as_deref().ok_or("an authenticated provider exchange needs --palw-producer-bond")?;
                    Some(PalwNativeExchangeHandleV3(Arc::new(AuthenticatedSocketExchangeV3 {
                        addr,
                        recovery_roster: ctx.recovery_provider_roster_v4.clone(),
                        policy: ProviderTransportPolicyV1::new(
                            transport_domain_v3(&ctx.config.params),
                            PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3,
                        ),
                        sender: PalwBondKeyV2(crate::palw_producer::parse_outpoint(bond)?),
                        key: kaspa_pq_validator_core::ValidatorKey::from_seed(kaspa_pq_validator_core::load_validator_seed(key)?),
                    })))
                }
                (None, Some(service), None) if !ctx.recovery_provider_roster_v4.is_empty() && ctx.producer_key_path.is_some() => {
                    // Explicit private V4 local service uses the SAME authenticated
                    // enrollment adapter as remote service; no unsigned local shortcut.
                    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
                    let addr = listener.local_addr().map_err(|e| e.to_string())?;
                    std::thread::Builder::new()
                        .name("palw-local-recovery-v4".into())
                        .spawn(move || serve_authenticated_v3(listener, service, PrivateNativeNetworkV3::Loopback))
                        .map_err(|e| e.to_string())?;
                    Some(PalwNativeExchangeHandleV3(Arc::new(AuthenticatedSocketExchangeV3 {
                        addr,
                        recovery_roster: ctx.recovery_provider_roster_v4.clone(),
                        policy: ProviderTransportPolicyV1::new(
                            transport_domain_v3(&ctx.config.params),
                            PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3,
                        ),
                        sender: PalwBondKeyV2(crate::palw_producer::parse_outpoint(
                            ctx.producer_bond.as_deref().ok_or("V4 local recovery producer bond absent")?,
                        )?),
                        key: kaspa_pq_validator_core::ValidatorKey::from_seed(kaspa_pq_validator_core::load_validator_seed(
                            ctx.producer_key_path.as_deref().unwrap(),
                        )?),
                    })))
                }
                (None, Some(service), None) => Some(PalwNativeExchangeHandleV3(service)),
                _ => None,
            })
        })
    }
}

/// The chain/network domain the transport binds: the attempt's own `network_domain`.
fn transport_domain_v3(params: &kaspa_consensus_core::config::params::Params) -> Hash64 {
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(params.net.to_string().as_bytes(), Some(params.genesis.hash))
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

struct ServedProviderV3 {
    bond: PalwBondKeyV2,
    operator: Hash64,
    key: kaspa_pq_validator_core::ValidatorKey,
    store: Mutex<DurableMonetaryMaterialProviderV1>,
    authorities: Mutex<AuthorityIndexV3>,
    janitor: Mutex<MonetaryMaterialGcJanitorV1>,
    recovery_acks: Mutex<misaka_palw_remote::material_recovery_ack_v2::MaterialRecoveryAckOutboxV2>,
}

/// **The in-node provider service.** Holds no consensus state of its own: every authority is a fresh Consensus API read.
pub struct LoopbackNativeProviderV3 {
    consensus_manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    config: Arc<kaspa_consensus_core::config::Config>,
    court: kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    holdings: Vec<misaka_palw_sdk::PalwLoadedArtifactV1>,
    chain_classes: bool,
    providers: Vec<ServedProviderV3>,
    transport: ProviderTransportPolicyV1,
    replay: Mutex<ProviderTransportReplayGuardV1>,
    additional_carriers: Mutex<misaka_palw_remote::native_carrier_queue_v4::NativeCarrierQueueV4>,
    additional_cursor: AtomicUsize,
    ack_cursor: AtomicUsize,
}

impl LoopbackNativeProviderV3 {
    fn open(ctx: &PalwNativeCustodyContextV3, keys: Vec<PrivateNativeProviderKeyV3>) -> Result<Self, String> {
        let params = &ctx.config.params;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return Err("a native provider needs a ConsensusV2 network".into());
        };
        let policy = &params.palw_monetary_material_v1.as_ref().ok_or("a native provider needs the monetary material fence")?.policy;
        let salt = ctx.config.palw_drill_genesis_salt.as_ref().ok_or("a native provider needs the salted private fixture")?;
        let operators: BTreeMap<PalwBondKeyV2, Hash64> = kaspa_consensus_core::config::drill::palw_t12_drill_cards_v1(salt)
            .into_iter()
            .map(|card| {
                (
                    PalwBondKeyV2(kaspa_consensus_core::config::drill::palw_t12_drill_premine_outpoint_v1(salt, card.premine_index)),
                    palw_operator_id_v2(&card.operator.pubkey),
                )
            })
            .collect();
        let dir = ctx.data_dir.join("palw-native-provider-v3");
        let mut providers = Vec::new();
        let metadata_limits = kaspa_consensus_core::palw_native_resource_v1::native_declared_logical_limits_v1()
            .map(|limits| {
                let n = keys.len().max(1) as u64;
                let orphan = kernel_ingress_v4::unhosted_metadata_reservation_v4(&dir, &keys)?;
                Ok::<_, String>(kaspa_consensus_core::palw_kernel_material_context_v4::PalwKernelMaterialContextLimitsV4 {
                    max_encoded_bytes: limits
                        .database_bytes
                        .checked_sub(orphan)
                        .ok_or("unhosted provider metadata exhausts explicit DB capacity")?
                        / n,
                    max_heap_bytes: limits.memory_bytes / n,
                })
            })
            .transpose()?;
        for key in keys {
            let file = dir.join(format!("{}-{}.store", key.bond.0.transaction_id, key.bond.0.index));
            let authorities = AuthorityIndexV3::open(&file.with_extension("authorities"))?;
            let store = if params.palw_material_release_recovery_v2.is_some() && metadata_limits.is_some() {
                DurableMonetaryMaterialProviderV1::open_with_kernel_contexts_v4(
                    &file,
                    policy.max_bytes_per_contract,
                    policy.max_bytes_per_provider,
                    |claim| authorities.get(&claim),
                    metadata_limits.unwrap(),
                )?
            } else {
                DurableMonetaryMaterialProviderV1::open(
                    &file,
                    policy.max_bytes_per_contract,
                    policy.max_bytes_per_provider,
                    None,
                    |claim| authorities.get(&claim),
                )?
            };
            let operator = *operators.get(&key.bond).ok_or("a native provider bond is not a fixture seat")?;
            let janitor = MonetaryMaterialGcJanitorV1::open(&file)?;
            let recovery_acks = misaka_palw_remote::material_recovery_ack_v2::MaterialRecoveryAckOutboxV2::open(
                &file.with_extension("recovery-ack-v2"),
            )?;
            providers.push(ServedProviderV3 {
                bond: key.bond,
                operator,
                key: key.key,
                store: Mutex::new(store),
                authorities: Mutex::new(authorities),
                janitor: Mutex::new(janitor),
                recovery_acks: Mutex::new(recovery_acks),
            });
        }
        let form = params.palw_prompt_ids_form_v1();
        let net = params.net.to_string().into_bytes();
        let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(bundle.court, form, net);
        let holdings = crate::palw_backends::load_class_holdings_v1(
            "palw-native-provider",
            &sdk,
            &ctx.class_artifacts,
            ctx.class_cache_bytes,
            misaka_palw_sdk::PalwWeightResidencyV1::PageCache,
        );
        Ok(Self {
            consensus_manager: Arc::clone(&ctx.consensus_manager),
            config: Arc::clone(&ctx.config),
            court: bundle.court,
            holdings,
            chain_classes: ctx.chain_classes,
            providers,
            additional_cursor: AtomicUsize::new(0),
            ack_cursor: AtomicUsize::new(0),
            additional_carriers: Mutex::new(misaka_palw_remote::native_carrier_queue_v4::NativeCarrierQueueV4::open(
                &dir.join("additional-carriers-v4"),
            )?),
            transport: ProviderTransportPolicyV1::new(transport_domain_v3(params), PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3),
            replay: Mutex::new(ProviderTransportReplayGuardV1::new(
                PALW_PROVIDER_TRANSPORT_REPLAY_CAPACITY_V1,
                misaka_palw_remote::provider_transport_v1::PALW_PROVIDER_TRANSPORT_MAX_SKEW_MS_V1,
            )),
        })
    }

    /// **Authenticate a network request before any work**: the envelope (domain, freshness, DAA window against this node's
    /// virtual DAA, the sender's signature, replay), then its binding to the body — the sender is the attempt's executor bond and
    /// key and the request is about the header's DAA — and the recipients are this service's bonds on the session roster. The
    /// executor key's registration is the authority read's (`sign_request`: it refuses a key that is not the bond's).
    fn authenticate(
        &self,
        request: &ProviderTransportRequestV1,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwNativeReadyRequestV3>, String> {
        let local_daa = Some(self.consensus_manager.consensus().unguarded_session().get_virtual_daa_score());
        {
            let mut replay = self.replay.lock().map_err(|_| "replay guard poisoned")?;
            provider_transport_verify_request_v1(
                request,
                ProviderTransportPurposeV1::NativeReadyV3,
                &self.transport,
                now_unix_ms(),
                local_daa,
                &mut replay,
            )?;
        }
        kaspa_consensus_core::palw_native_resource_v1::preflight_native_value_v1::<PalwNativeReadyRequestV3>(
            &request.body,
            request.body.len() as u64,
        )
        .map_err(|e| e.to_string())?;
        let native = kaspa_consensus_core::palw_native_resource_v1::retain_native_value_decode_v1(&request.body, || {
            borsh::from_slice::<PalwNativeReadyRequestV3>(&request.body)
        })
        .map_err(|e| e.to_string())?;
        let attempt = &native.envelope.base.attempt;
        let header = &request.header;
        if header.sender.0 != attempt.executor_bond
            || header.sender_pubkey != attempt.executor_pubkey
            || attempt.network_domain != self.transport.domain
            || header.daa_score != native.header.daa_score
        {
            return Err("the authenticated sender is not the attempt's executor bond/key, or the request names another DAA".into());
        }
        if !header.recipients.iter().all(|bond| native.envelope.ready.providers.contains(bond))
            || !self.providers.iter().any(|p| header.recipients.contains(&p.bond))
        {
            return Err("the request's recipients are not this service's bonds on the session roster".into());
        }
        Ok(native)
    }

    /// The signed response: every bond this service holds among the request's recipients signs it.
    fn respond(
        &self,
        request: &ProviderTransportRequestV1,
        body: &PalwNativeReadyResponseV3,
    ) -> Result<ProviderTransportResponseV1, String> {
        let signers: Vec<_> = self
            .providers
            .iter()
            .filter(|p| request.header.recipients.contains(&p.bond))
            .map(|p| {
                (p.bond, move |message: &[u8], context: &[u8]| -> Result<Vec<u8>, String> {
                    Ok(p.key.sign_with_context(message, context).to_vec())
                })
            })
            .collect();
        let responders: Vec<(PalwBondKeyV2, ProviderTransportSignerV1<'_>)> =
            signers.iter().map(|(bond, sign)| (*bond, sign as ProviderTransportSignerV1<'_>)).collect();
        provider_transport_response_v1(&request.header, now_unix_ms(), borsh::to_vec(body).map_err(|e| e.to_string())?, responders)
    }

    /// **One janitor round per served bond** over this node's Consensus API (see the module comment). Logged, never fatal.
    pub fn gc_tick(&self) {
        if let Err(error) = self.answer_and_release_tick_v4() {
            kaspa_core::warn!("RFC08 V4 provider consumer retained for retry: {error}");
        }
        for provider in &self.providers {
            let (Ok(mut janitor), Ok(mut store)) = (provider.janitor.lock(), provider.store.lock()) else {
                kaspa_core::warn!("RFC12 PRIVATE: PAV3 provider GC skipped a poisoned store; everything retained");
                continue;
            };
            let mut oracle = ConsensusGcOracleV3(&self.consensus_manager);
            let native = self.consensus_manager.consensus().unguarded_session_blocking();
            if let Err(error) = store.expire_unused_kernel_offer_tick_v4(&*native) {
                kaspa_core::warn!("RFC08 kernel unused-offer metadata retained for retry: {error}");
            }
            match janitor.tick(&mut store, &mut oracle) {
                Ok(report) => {
                    if !report.deleted.is_empty() {
                        kaspa_core::info!(
                            "RFC12 PRIVATE: PAV3 provider {}:{} deleted {} finalized-release bundle(s); {} retained",
                            provider.bond.0.transaction_id,
                            provider.bond.0.index,
                            report.deleted.len(),
                            report.retained
                        );
                    }
                    if let Some(reason) = report.refused {
                        kaspa_core::warn!("RFC12 PRIVATE: PAV3 provider GC retained everything this round: {reason}");
                    }
                }
                Err(error) => kaspa_core::warn!(
                    "RFC12 PRIVATE: PAV3 provider GC stopped; verified prior deletions may be durable, other material retained: {error}"
                ),
            }
            // Receipts survive store deletion, marker replay and restart. Never
            // infer cleanup authority from a missing bundle alone.
            let receipts: Vec<_> = janitor.completed_receipts().cloned().collect();
            let still_held: BTreeSet<_> = store.held_native_authorities().into_values().flatten().collect();
            let mut recovery_roots = BTreeMap::new();
            let mut metadata_clean = true;
            for receipt in &receipts {
                let retired: BTreeSet<_> = receipt.native_claims.iter().copied().filter(|claim| !still_held.contains(claim)).collect();
                let result = provider
                    .authorities
                    .lock()
                    .map_err(|_| "authority metadata lock poisoned".to_string())
                    .and_then(|mut index| index.remove_finalized(&retired));
                match result {
                    Ok(()) => match oracle.terminal_receipt(receipt.intent.root, receipt.intent.stored_consent) {
                        Ok(Some((_, terminal))) => {
                            recovery_roots.insert(terminal.id(), terminal);
                        }
                        Ok(None) => {
                            if let Err(error) = janitor.acknowledge_metadata_cleanup(receipt.intent.stored_consent) {
                                kaspa_core::warn!("RFC12 PRIVATE: finalized authority metadata ack retained: {error}");
                            }
                        }
                        Err(error) => {
                            metadata_clean = false;
                            kaspa_core::warn!("RFC12 PRIVATE: recovery receipt read retained for retry: {error}");
                        }
                    },
                    Err(error) => {
                        metadata_clean = false;
                        kaspa_core::warn!("RFC12 PRIVATE: finalized provider metadata retained for retry: {error}");
                    }
                }
            }
            let mut _terminal_receipts_lease = None;
            if metadata_clean {
                match self
                    .consensus_manager
                    .consensus()
                    .unguarded_session_blocking()
                    .palw_material_provider_terminal_receipts_finalized_v4(provider.bond)
                {
                    Ok(rows) => {
                        let (rows, lease) = rows.into_parts_v1();
                        _terminal_receipts_lease = lease;
                        for terminal in rows {
                            if terminal.unused_native_session {
                                recovery_roots.insert(terminal.id(), terminal);
                            }
                        }
                    }
                    Err(error) => {
                        kaspa_core::warn!("RFC12 PRIVATE: terminal recovery scan retained: {error}");
                        continue;
                    }
                }
                if let Ok(mut outbox) = provider.recovery_acks.lock() {
                    let Ok(additional) = self.additional_carriers.lock() else { continue };
                    if additional.provider_in_flight(provider.bond) {
                        continue;
                    }

                    for terminal in recovery_roots.into_values() {
                        if terminal.acknowledged.contains(&provider.bond) {
                            continue;
                        }
                        let checkpoint = match self
                            .consensus_manager
                            .consensus()
                            .unguarded_session_blocking()
                            .palw_material_provider_ack_checkpoint_finalized_v2(provider.bond)
                        {
                            Ok(Some((_, checkpoint))) => checkpoint,
                            Ok(None) => {
                                kaspa_core::warn!("RFC12 PRIVATE: provider ACK checkpoint not finalized yet");
                                continue;
                            }
                            Err(error) => {
                                kaspa_core::warn!("RFC12 PRIVATE: provider ACK checkpoint unknown: {error}");
                                continue;
                            }
                        };
                        if let Err(error) = outbox.enqueue_after_durable_gc(
                            self.transport.domain,
                            &terminal,
                            provider.bond,
                            &checkpoint,
                            &receipts,
                            &store.held_contracts(),
                            |bond, message, context| {
                                (*bond == provider.bond).then(|| provider.key.sign_with_context(message, context).to_vec())
                            },
                        ) {
                            kaspa_core::warn!(
                                "RFC12 PRIVATE: recovery ACK waits for all generation deletions/funding adapter: {error}"
                            );
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn pending_additional_carrier_v4(
        &self,
    ) -> Result<Option<super::recovery_ack_carrier_v4::AdditionalNativeCarrierV4>, String> {
        let queue = self.additional_carriers.lock().map_err(|_| "additional carrier queue poisoned")?;
        let count = queue.rows().count();
        if count == 0 {
            return Ok(None);
        }
        let index = self.additional_cursor.fetch_add(1, Ordering::Relaxed) % count;
        let (id, row) = queue.rows().nth(index).ok_or("additional carrier cursor changed under lock")?;
        Ok(Some(super::recovery_ack_carrier_v4::AdditionalNativeCarrierV4 {
            id: *id,
            object: row.object_owned()?,
            submitted_transaction: row.transaction_owned()?,
        }))
    }
    pub(crate) fn record_additional_carrier_submitted_v4(
        &self,
        id: Hash64,
        tx: &kaspa_consensus_core::tx::Transaction,
    ) -> Result<(), String> {
        self.additional_carriers.lock().map_err(|_| "additional carrier queue poisoned")?.record_submitted(id, tx)
    }
    pub(crate) fn retire_additional_carrier_finalized_v4(&self, id: Hash64) -> Result<bool, String> {
        use misaka_palw_remote::native_carrier_queue_v4::NativeCarrierPurposeV4;
        let mut queue = self.additional_carriers.lock().map_err(|_| "additional carrier queue poisoned")?;
        let Some(row) = queue.get(&id) else { return Ok(true) };
        let api = self.consensus_manager.consensus().unguarded_session_blocking();
        let applied = match row.purpose {
            NativeCarrierPurposeV4::Enrollment { root, consent, recovery, until_daa, .. }
            | NativeCarrierPurposeV4::KernelEnrollmentV4 { root, consent, recovery, until_daa }
            | NativeCarrierPurposeV4::KernelReplacementV4 { root, consent, recovery, until_daa } => {
                api.palw_material_recovery_enrollment_finalized_v4(root, consent, recovery)?
                    || api.palw_material_recovery_finalized_daa_v4()?.is_some_and(|daa| daa > until_daa)
            }
            NativeCarrierPurposeV4::Answer { provider, stream, sequence, event, deadline_daa } => {
                api.palw_material_provider_ack_checkpoint_finalized_v2(provider)?
                    .is_some_and(|(_, c)| c.stream == stream && c.sequence == sequence && c.last_receipt == event)
                    || api.palw_material_recovery_finalized_daa_v4()?.is_some_and(|daa| daa > deadline_daa)
            }
            NativeCarrierPurposeV4::Release { root, consent, .. } => {
                api.palw_material_terminal_receipt_finalized_v2(root, consent)?.is_some()
            }
        };
        // Expiry retires ONLY this immutable, now impossible carrier metadata;
        // held bytes, covenants, challenges and earned cash remain native-owned.
        if applied {
            queue.retire(id)?;
        }
        Ok(applied)
    }
    /// Pending signed ACKs are read by the explicitly configured private fee
    /// payer. Submission is recorded before network send and retained to finality.
    pub fn pending_recovery_ack_v2(
        &self,
    ) -> Result<Option<(PalwBondKeyV2, misaka_palw_remote::material_recovery_ack_v2::MaterialRecoveryAckSummaryV2)>, String> {
        if self.providers.is_empty() {
            return Ok(None);
        }
        let start = self.ack_cursor.fetch_add(1, Ordering::Relaxed) % self.providers.len();
        for offset in 0..self.providers.len() {
            let p = &self.providers[(start + offset) % self.providers.len()];
            let outbox = p.recovery_acks.lock().map_err(|_| "ACK outbox poisoned")?;
            if let Some(row) = outbox.pending().next() {
                return Ok(Some((
                    p.bond,
                    misaka_palw_remote::material_recovery_ack_v2::MaterialRecoveryAckSummaryV2 { receipt: row.receipt },
                )));
            }
        }
        Ok(None)
    }
    pub fn recovery_ack_object_owned_v2(
        &self,
        provider: PalwBondKeyV2,
        receipt: Hash64,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
        String,
    > {
        let p = self.providers.iter().find(|p| p.bond == provider).ok_or("unknown ACK provider")?;
        let outbox = p.recovery_acks.lock().map_err(|_| "ACK outbox lock poisoned")?;
        outbox.pending().find(|row| row.receipt == receipt).ok_or("unknown ACK receipt")?.object_owned_v2()
    }
    pub fn recovery_ack_transaction_owned_v2(
        &self,
        provider: PalwBondKeyV2,
        receipt: Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Vec<u8>>>, String> {
        let p = self.providers.iter().find(|p| p.bond == provider).ok_or("unknown ACK provider")?;
        let outbox = p.recovery_acks.lock().map_err(|_| "ACK outbox poisoned")?;
        outbox.pending().find(|r| r.receipt == receipt).ok_or("unknown ACK receipt")?.transaction_owned_v2()
    }
    pub fn record_recovery_ack_submitted_v2(
        &self,
        provider: PalwBondKeyV2,
        receipt: Hash64,
        tx: &kaspa_consensus_core::tx::Transaction,
    ) -> Result<(), String> {
        self.providers
            .iter()
            .find(|p| p.bond == provider)
            .ok_or("unknown ACK provider")?
            .recovery_acks
            .lock()
            .map_err(|_| "ACK outbox lock poisoned")?
            .record_submitted(receipt, tx)
    }
    pub fn record_recovery_ack_accepted_v2(
        &self,
        provider: PalwBondKeyV2,
        receipt: Hash64,
        block: kaspa_consensus_core::BlockHash,
    ) -> Result<(), String> {
        self.providers
            .iter()
            .find(|p| p.bond == provider)
            .ok_or("unknown ACK provider")?
            .recovery_acks
            .lock()
            .map_err(|_| "ACK outbox lock poisoned")?
            .record_accepted(receipt, block)
    }
    /// Current finalized ACK stream presence proves ACTUAL state application,
    /// including after last-ACK terminal deletion and historical body/delta pruning.
    pub fn retire_recovery_ack_checkpoint_v2(&self, provider: PalwBondKeyV2, receipt: Hash64) -> Result<bool, String> {
        let p = self.providers.iter().find(|p| p.bond == provider).ok_or("unknown ACK provider")?;
        let Some((_, checkpoint)) = self
            .consensus_manager
            .consensus()
            .unguarded_session_blocking()
            .palw_material_provider_ack_checkpoint_finalized_v2(provider)?
        else {
            return Ok(false);
        };
        let mut janitor = p.janitor.lock().map_err(|_| "ACK janitor lock poisoned")?;
        let mut outbox = p.recovery_acks.lock().map_err(|_| "ACK outbox lock poisoned")?;
        let mut applied = false;
        outbox.retire_checkpoint_finalized(receipt, |row| {
            applied = checkpoint.provider == row.provider
                && checkpoint.stream == row.stream
                && checkpoint.sequence == row.sequence
                && checkpoint.last_receipt == row.receipt;
            if applied {
                for consent in &row.stored_consents {
                    janitor.acknowledge_metadata_cleanup(*consent)?;
                }
            }
            Ok(applied)
        })?;
        Ok(applied)
    }
    pub fn retire_recovery_ack_finalized_v2(
        &self,
        provider: PalwBondKeyV2,
        receipt: Hash64,
        verify: impl FnMut(
            &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
            &[u8],
            kaspa_consensus_core::BlockHash,
        ) -> Result<bool, String>,
    ) -> Result<(), String> {
        let p = self.providers.iter().find(|p| p.bond == provider).ok_or("unknown ACK provider")?;
        {
            let outbox = p.recovery_acks.lock().map_err(|_| "ACK outbox poisoned")?;
            let row = outbox.pending().find(|r| r.receipt == receipt).ok_or("unknown ACK receipt")?;
            let Some(submission) = &row.submission else { return Ok(()) };
            let Some(block) = submission.accepted_block else { return Ok(()) };
            let object = row.object_owned_v2()?;
            let mut verify = verify;
            if !verify(&object, &submission.transaction, block)? {
                return Ok(());
            }
        }
        // Historical inclusion is merely an additional check. Only actual
        // current native-finalized checkpoint presence authorizes retirement.
        self.retire_recovery_ack_checkpoint_v2(provider, receipt).map(|_| ())
    }

    /// One request, every roster bond this service holds: each signature only after its own durable publication.
    pub fn sign_request(
        &self,
        request: &PalwNativeReadyRequestV3,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Vec<(PalwBondKeyV2, Vec<u8>)>>, String> {
        let requests = (self.providers.len() as u64)
            .checked_mul(
                (std::mem::size_of::<(PalwBondKeyV2, Vec<u8>)>() + 6 * kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)
                    as u64,
            )
            .ok_or("Ready signature workspace overflow")?;
        let _signature_workspace =
            kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(requests).map_err(|e| e.to_string())?;
        if request.version != PALW_NATIVE_READY_EXCHANGE_VERSION_V3 {
            return Err("unknown native exchange version".into());
        }
        let ready = &request.envelope.ready;
        let attempt = &request.envelope.base.attempt;
        let ours: Vec<&ServedProviderV3> = self.providers.iter().filter(|p| ready.providers.contains(&p.bond)).collect();
        if ours.is_empty() {
            return Err("none of this service's provider bonds is on the session roster".into());
        }
        if ours.iter().any(|p| p.operator == attempt.operator_id) {
            return Err("the executor is one of this service's provider operators; self-custody is refused".into());
        }
        if ready
            .binding
            .manifest
            .scopes
            .iter()
            .any(|scope| scope.codec == kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldCompleteV2)
        {
            return self.sign_routed_request_v2(request, &ours);
        }
        let session = self.consensus_manager.consensus().unguarded_session();
        let authority = session.palw_native_material_provider_authority_v3(request.checkpoint, &request.header, &request.envelope)?;
        let params = &self.config.params;
        let registry = crate::palw_backends::PalwBackendRegistry::for_node_v1(
            params,
            self.court,
            params.palw_prompt_ids_form_v1(),
            self.holdings.clone(),
            params.net.to_string().into_bytes(),
            self.chain_classes,
        );
        let backend = registry.resolve_or_chain(attempt.class_id, attempt.artifact_root, |id| {
            if self.chain_classes { session.palw_registered_class_carriage_v1(id) } else { None }
        })?;
        drop(session);
        // The authority answer validated the attempt's network domain, position and signature: the anchor is the header's.
        let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(
            attempt.network_domain,
            kaspa_consensus_core::hashing::header::pre_pow_hash_64(&request.header),
            attempt.class_id,
            &attempt.executor_bond,
            request.header.nonce,
        );
        let prompt = palw_native_prompt_v3(backend.as_ref(), anchor)?;
        if prompt != request.prompt {
            return Err("the request's prompt is not the one its anchor implies".into());
        }
        let prefill_draw = params.palw_prefill_draw_active_at(request.header.daa_score);
        let (bundle, manifest) = palw_native_bundle_v3(
            backend.as_ref(),
            &authority.row,
            attempt,
            anchor,
            &request.capture,
            &prompt,
            prefill_draw,
            authority.material,
        )?;
        if manifest != ready.binding.manifest {
            return Err("the producer's manifest differs from the bundle this provider rebuilt".into());
        }
        let mut signatures = Vec::new();
        for provider in ours {
            let (_, registered) = authority
                .provider_keys
                .iter()
                .find(|(bond, _)| *bond == provider.bond)
                .ok_or("the chain names no key for this roster bond")?;
            if registered.as_slice() != provider.key.public_key() {
                return Err("the chain's registered key for this roster bond is not the key this service holds".into());
            }
            // The reopen index first: a crash after publication must still find the authority to re-verify the bundle.
            // One lock order for sign and GC: store then authority index. Keep
            // it across authority publication + material publication, otherwise
            // GC could erase a new authority while its Ready waits on the store.
            let mut store = provider.store.lock().map_err(|_| "provider store poisoned")?;
            let mut authorities = provider.authorities.lock().map_err(|_| "authority index poisoned")?;
            authorities.record(bundle.claim, authority.material)?;
            let signature = misaka_palw_remote::native_ready_v3::provider_sign_native_ready_v3(
                &mut store,
                provider.bond,
                &authority.row,
                ready,
                &bundle,
                authority.material,
                |message, context| Ok(provider.key.sign_with_context(message, context).to_vec()),
            )?;
            signatures.push((provider.bond, signature));
        }
        let encoded = borsh::to_vec(&signatures).map_err(|e| e.to_string())?;
        kaspa_consensus_core::palw_native_resource_v1::preflight_native_value_v1::<Vec<(PalwBondKeyV2, Vec<u8>)>>(
            &encoded,
            encoded.len() as u64,
        )
        .map_err(|e| e.to_string())?;
        kaspa_consensus_core::palw_native_resource_v1::retain_native_value_decode_v1(&encoded, || {
            borsh::from_slice::<Vec<(PalwBondKeyV2, Vec<u8>)>>(&encoded)
        })
        .map_err(|e| e.to_string())
    }
    fn sign_routed_request_v2(
        &self,
        request: &PalwNativeReadyRequestV3,
        ours: &[&ServedProviderV3],
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Vec<(PalwBondKeyV2, Vec<u8>)>>, String> {
        use kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1;
        let ready = &request.envelope.ready;
        let attempt = &request.envelope.base.attempt;
        if ready.binding.manifest.scopes.len() != 1
            || ready.binding.manifest.scopes[0].codec != PalwMaterialCodecV1::NativeHeldCompleteV2
        {
            return Err("V2 native Ready requires exactly its one explicit routed scope".into());
        }
        let session = self.consensus_manager.consensus().unguarded_session();
        let authority = session.palw_native_material_provider_authority_v4(request.checkpoint, &request.header, &request.envelope)?;
        let params = &self.config.params;
        let registry = crate::palw_backends::PalwBackendRegistry::for_node_v1(
            params,
            self.court,
            params.palw_prompt_ids_form_v1(),
            self.holdings.clone(),
            params.net.to_string().into_bytes(),
            self.chain_classes,
        );
        let backend = registry.resolve_or_chain(attempt.class_id, attempt.artifact_root, |id| {
            if self.chain_classes { session.palw_registered_class_carriage_v1(id) } else { None }
        })?;
        drop(session);
        let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(
            attempt.network_domain,
            kaspa_consensus_core::hashing::header::pre_pow_hash_64(&request.header),
            attempt.class_id,
            &attempt.executor_bond,
            request.header.nonce,
        );
        let prompt = palw_native_prompt_v3(backend.as_ref(), anchor)?;
        if prompt != request.prompt {
            return Err("V2 native request prompt differs from own anchor job".into());
        }
        let (bundle, manifest) = crate::palw_native_custody_v3::palw_native_bundle_routed_v2(
            backend.as_ref(),
            &authority.row,
            attempt,
            anchor,
            &request.capture,
            &prompt,
            params.palw_prefill_draw_active_at(request.header.daa_score),
            &authority.material,
        )
        .map_err(|e| e.to_string())?;
        if manifest != ready.binding.manifest {
            return Err("V2 native provider rebuilt another manifest".into());
        }
        // The signatures, encoded handoff and returned decoded owner overlap.
        // Reserve their exact declared types/counts before creating the new Vecs.
        let signature_bytes = kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
        let mut publication = kaspa_consensus_core::palw_routed_material_v2::PalwMaterialConstructionRequestsV2::new();
        publication.vec_v1::<(PalwBondKeyV2, Vec<u8>)>(ours.len() as u64).map_err(|e| e.to_string())?;
        let encoded = ours
            .len()
            .checked_mul(68 + 4 + signature_bytes)
            .and_then(|n| n.checked_add(4))
            .ok_or("V2 provider signature framing overflow")?;
        publication.vec_v1::<u8>(encoded as u64).map_err(|e| e.to_string())?;
        for _ in ours {
            publication.vec_v1::<u8>(signature_bytes as u64).map_err(|e| e.to_string())?;
        }
        let _publication = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(publication.bytes_v1())
            .map_err(|e| e.to_string())?;
        let mut signatures = Vec::with_capacity(ours.len());
        for provider in ours {
            let (_, registered) =
                authority.provider_keys.iter().find(|(bond, _)| *bond == provider.bond).ok_or("V2 provider native key absent")?;
            if registered.as_slice() != provider.key.public_key() {
                return Err("V2 provider key differs from own accepted bond".into());
            }
            // Re-read actual own candidate authority before publication/signing.
            // A persisted pre-admission row never replaces the native reader.
            let session = self.consensus_manager.consensus().unguarded_session();
            let current =
                session.palw_native_material_provider_authority_v4(request.checkpoint, &request.header, &request.envelope)?;
            if current.material.record() != authority.material.record() || current.row != authority.row {
                return Err("V2 native candidate changed before provider publication".into());
            }
            drop(session);
            let mut store = provider.store.lock().map_err(|_| "provider store poisoned")?;
            let signature = misaka_palw_remote::native_ready_v3::provider_sign_native_ready_routed_v2(
                &mut store,
                provider.bond,
                &current.row,
                ready,
                &bundle,
                &current.material,
                |message, context| Some(provider.key.sign_with_context(message, context).to_vec()),
            )
            .map_err(|e| e.to_string())?;
            signatures.push((provider.bond, signature));
        }
        let encoded = borsh::to_vec(&signatures).map_err(|e| e.to_string())?;
        kaspa_consensus_core::palw_native_resource_v1::retain_native_value_decode_v1(&encoded, || {
            borsh::from_slice::<Vec<(PalwBondKeyV2, Vec<u8>)>>(&encoded)
        })
        .map_err(|e| e.to_string())
    }
}

impl NativeReadyExchangeV3 for LoopbackNativeProviderV3 {
    fn describe(&self) -> String {
        format!("in-process loopback provider ({} bond(s))", self.providers.len())
    }
    fn request_signatures(
        &self,
        request: &PalwNativeReadyRequestV3,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<BTreeMap<PalwBondKeyV2, Vec<u8>>>, String> {
        crate::palw_native_custody_v3::palw_native_signature_map_owned_v3(&self.sign_request(request)?)
    }
}

/// **The client half**: one connection per request to a provider service on this or another host, the request signed with
/// this node's producer key and every response verified with the roster keys this node's own authority read returned.
pub struct AuthenticatedSocketExchangeV3 {
    pub addr: SocketAddr,
    policy: ProviderTransportPolicyV1,
    sender: PalwBondKeyV2,
    key: kaspa_pq_validator_core::ValidatorKey,
    recovery_roster: Vec<PalwBondKeyV2>,
}

/// A request nonce: unique, not secret (replay protection needs uniqueness per sender only).
fn request_nonce_v3(body: &[u8]) -> [u8; 32] {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut state = blake2b_simd::Params::new().hash_length(32).key(b"MISAKA/PALW/PROVIDER-TRANSPORT/NONCE/V1").to_state();
    state.update(&nanos.to_le_bytes());
    state.update(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    state.update(&std::process::id().to_le_bytes());
    state.update(body);
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(state.finalize().as_bytes());
    nonce
}

impl NativeReadyExchangeV3 for AuthenticatedSocketExchangeV3 {
    fn replace_kernel_material_v4(
        &self,
        context: &kaspa_consensus_core::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
        bundle: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
        intent: &kaspa_consensus_core::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementIntentV4,
        grant: &kaspa_consensus_core::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementOwnedV4,
    ) -> Result<(), String> {
        self.replace_kernel_v4(context, bundle, intent, grant)
    }
    fn fetch_kernel_material_unit_classified_v4(
        &self,
        query: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialUnitQueryV4,
        authority: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialQueryAuthorityV4,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Vec<u8>>,
        kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialFetchErrorV4,
    > {
        self.fetch_kernel_classified_v4(query, authority)
    }
    fn fetch_kernel_material_unit_v4(
        &self,
        query: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialUnitQueryV4,
        authority: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialQueryAuthorityV4,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Vec<u8>>, String> {
        self.fetch_kernel_v4(query, authority)
    }
    fn ingest_kernel_material_v4(
        &self,
        material: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
        ingress: &kaspa_consensus_core::palw_kernel_material_enrollment_v4::PalwKernelMaterialIngressOwnedV4,
    ) -> Result<(), String> {
        self.ingest_kernel_v4(material, ingress)
    }
    fn recovery_provider_roster_v4(&self) -> &[PalwBondKeyV2] {
        &self.recovery_roster
    }
    fn ensure_recovery_session_v4(
        &self,
        q: &kaspa_consensus_core::palw_material_session_enrollment_v4::PalwNativeRecoverySessionQuoteV4,
    ) -> Result<(), String> {
        self.enroll_recovery_session_v4(q)
    }
    fn describe(&self) -> String {
        format!("authenticated provider socket {} (integrity-only, unencrypted)", self.addr)
    }
    fn request_signatures(
        &self,
        _request: &PalwNativeReadyRequestV3,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<BTreeMap<PalwBondKeyV2, Vec<u8>>>, String> {
        Err("an authenticated provider socket verifies responses with the roster keys; no unauthenticated path exists".into())
    }
    fn request_signatures_authenticated(
        &self,
        request: &PalwNativeReadyRequestV3,
        roster_keys: &[(PalwBondKeyV2, Vec<u8>)],
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<BTreeMap<PalwBondKeyV2, Vec<u8>>>, String> {
        let attempt = &request.envelope.base.attempt;
        if attempt.executor_bond != self.sender.0 || attempt.executor_pubkey.as_slice() != self.key.public_key() {
            return Err("this node's producer key/bond is not the attempt's executor".into());
        }
        let body = borsh::to_vec(request).map_err(|e| e.to_string())?;
        let signed = provider_transport_request_v1(
            ProviderTransportPurposeV1::NativeReadyV3,
            self.policy.domain,
            self.sender,
            self.key.public_key().to_vec(),
            request.envelope.ready.providers.clone(),
            request_nonce_v3(&body),
            now_unix_ms(),
            request.header.daa_score,
            body,
            |message, context| Ok(self.key.sign_with_context(message, context).to_vec()),
        )?;
        let mut stream = TcpStream::connect_timeout(&self.addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
        stream.set_read_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        write_frame_v3(&mut stream, &borsh::to_vec(&signed).map_err(|e| e.to_string())?)?;
        let frame = read_frame_v3(&mut stream)?;
        kaspa_consensus_core::palw_native_resource_v1::preflight_native_value_v1::<ProviderTransportResponseV1>(
            &frame,
            frame.len() as u64,
        )
        .map_err(|e| e.to_string())?;
        let response = kaspa_consensus_core::palw_native_resource_v1::retain_native_value_decode_v1(&frame, || {
            borsh::from_slice::<ProviderTransportResponseV1>(&frame)
        })
        .map_err(|e| e.to_string())?;
        let responders = provider_transport_verify_response_v1(&response, &signed.header, roster_keys, &self.policy, now_unix_ms())?;
        kaspa_consensus_core::palw_native_resource_v1::preflight_native_value_v1::<PalwNativeReadyResponseV3>(
            &response.body,
            response.body.len() as u64,
        )
        .map_err(|e| e.to_string())?;
        let body = kaspa_consensus_core::palw_native_resource_v1::retain_native_value_decode_v1(&response.body, || {
            borsh::from_slice::<PalwNativeReadyResponseV3>(&response.body)
        })
        .map_err(|e| e.to_string())?;
        match &*body {
            PalwNativeReadyResponseV3::Signed { version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3, signatures } => {
                if !signatures.iter().map(|(b, _)| b).eq(responders.iter()) {
                    return Err("the provider service's Ready signatures are not exactly its authenticated responders".into());
                }
                crate::palw_native_custody_v3::palw_native_signature_map_owned_v3(signatures)
            }
            PalwNativeReadyResponseV3::Refused { reason, .. } => Err(format!("provider refused (authenticated): {reason}")),
            PalwNativeReadyResponseV3::Signed { .. } => Err("unknown native exchange response version".into()),
        }
    }
}

/// **The service half.** Loopback mode admits loopback peers only; remote mode any peer. At most
/// [`PRIVATE_NATIVE_EXCHANGE_MAX_CONNECTIONS_V3`] connections at once, one request each. An unauthenticated request gets no
/// answer (the service is never a signing oracle for unauthenticated bytes); an authenticated one gets a signed answer.
fn serve_authenticated_v3(listener: TcpListener, service: Arc<LoopbackNativeProviderV3>, network: PrivateNativeNetworkV3) {
    let in_flight = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let admitted = match network {
            PrivateNativeNetworkV3::Loopback => stream.peer_addr().is_ok_and(|peer| peer.ip().is_loopback()),
            PrivateNativeNetworkV3::Remote => stream.peer_addr().is_ok(),
        };
        if !admitted || in_flight.fetch_add(1, Ordering::AcqRel) >= PRIVATE_NATIVE_EXCHANGE_MAX_CONNECTIONS_V3 {
            if admitted {
                in_flight.fetch_sub(1, Ordering::AcqRel);
            }
            continue;
        }
        let (service, counter) = (Arc::clone(&service), Arc::clone(&in_flight));
        let spawned = std::thread::Builder::new().name("palw-native-provider-conn-v3".into()).spawn(move || {
            handle_authenticated_v3(stream, &service);
            counter.fetch_sub(1, Ordering::AcqRel);
        });
        if spawned.is_err() {
            in_flight.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

fn handle_authenticated_v3(mut stream: TcpStream, service: &LoopbackNativeProviderV3) {
    let _ = stream.set_read_timeout(Some(PRIVATE_NATIVE_EXCHANGE_IDLE_TIMEOUT_V3));
    let _ = stream.set_write_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3));
    let Ok(bytes) = read_frame_v3(&mut stream) else { return };
    if kaspa_consensus_core::palw_native_resource_v1::preflight_native_value_v1::<ProviderTransportRequestV1>(
        &bytes,
        bytes.len() as u64,
    )
    .is_err()
    {
        return;
    }
    let Ok(request) = kaspa_consensus_core::palw_native_resource_v1::retain_native_value_decode_v1(&bytes, || {
        borsh::from_slice::<ProviderTransportRequestV1>(&bytes)
    }) else {
        return;
    };
    if request.header.purpose == ProviderTransportPurposeV1::RecoveryEnrollmentV4 {
        recovery_runtime_v4::handle_recovery_v4(&mut stream, service, &request);
        return;
    }
    if request.header.purpose == ProviderTransportPurposeV1::KernelMaterialIngressV4 {
        kernel_ingress_v4::handle_kernel_v4(&mut stream, service, &request);
        return;
    }
    if request.header.purpose == ProviderTransportPurposeV1::KernelMaterialQueryV4 {
        kernel_query_v4::handle_query_v4(&mut stream, service, &request);
        return;
    }
    if request.header.purpose == ProviderTransportPurposeV1::KernelMaterialReplacementV4 {
        kernel_replacement_v4::handle_replacement_v4(&mut stream, service, &request);
        return;
    }
    let native = match service.authenticate(&request) {
        Ok(native) => native,
        Err(reason) => {
            kaspa_core::warn!("RFC12 PRIVATE: PAV3 native provider dropped an unauthenticated request: {reason}");
            return;
        }
    };
    let (body, _signature_lease) = match service.sign_request(&native) {
        Ok(signatures) => {
            let (signatures, lease) = signatures.into_parts_v1();
            (PalwNativeReadyResponseV3::Signed { version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3, signatures }, lease)
        }
        Err(reason) => {
            kaspa_core::warn!("RFC12 PRIVATE: PAV3 native provider refused a request: {reason}");
            (PalwNativeReadyResponseV3::Refused { version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3, reason }, None)
        }
    };
    match service.respond(&request, &body).and_then(|response| borsh::to_vec(&response).map_err(|e| e.to_string())) {
        Ok(bytes) => {
            let _ = write_frame_v3(&mut stream, &bytes);
        }
        Err(reason) => kaspa_core::warn!("RFC12 PRIVATE: PAV3 native provider could not sign its response: {reason}"),
    }
}

/// The node's own Consensus API as the janitor's oracle: coherent covenant read (sink unchanged across it), the selected
/// chain after a point, and the physical-GC witness.
struct ConsensusGcOracleV3<'a>(&'a kaspa_consensusmanager::ConsensusManager);

impl MonetaryMaterialGcOracleV1 for ConsensusGcOracleV3<'_> {
    fn terminal_receipt(
        &mut self,
        root: Hash64,
        stored: Hash64,
    ) -> Result<
        Option<(
            kaspa_consensus_core::BlockHash,
            kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialTerminalReceiptV2,
        )>,
        String,
    > {
        self.0.consensus().unguarded_session_blocking().palw_material_terminal_receipt_finalized_v2(root, stored)
    }

    fn covenant_rows(
        &mut self,
    ) -> Result<
        (kaspa_consensus_core::BlockHash, Vec<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1>),
        String,
    > {
        let session = self.0.consensus().unguarded_session_blocking();
        let before = session.get_sink();
        let rows = session.palw_monetary_material_covenants_v1()?;
        if session.get_sink() != before {
            return Err("the sink moved during the covenant read".into());
        }
        Ok((before, rows))
    }
    fn chain_after(
        &mut self,
        low: kaspa_consensus_core::BlockHash,
        limit: usize,
    ) -> Result<Vec<kaspa_consensus_core::BlockHash>, String> {
        let session = self.0.consensus().unguarded_session_blocking();
        session.get_virtual_chain_from_block(low, Some(limit)).map(|path| path.added).map_err(|e| e.to_string())
    }
    fn release_finalized(
        &mut self,
        root: Hash64,
        consent: Hash64,
        tail: Hash64,
        release_block: kaspa_consensus_core::BlockHash,
    ) -> Result<bool, String> {
        let session = self.0.consensus().unguarded_session_blocking();
        if let Some((_, receipt)) = session.palw_material_terminal_receipt_finalized_v2(root, consent)? {
            return Ok(receipt.tail_block == tail);
        }
        session.palw_monetary_material_release_finalized_v1(root, consent, tail, release_block)
    }
}

fn write_frame_v3(stream: &mut impl Write, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3 {
        return Err("native exchange frame exceeds its cap".into());
    }
    stream
        .write_all(&(bytes.len() as u64).to_le_bytes())
        .and_then(|_| stream.write_all(bytes))
        .and_then(|_| stream.flush())
        .map_err(|e| e.to_string())
}

struct NativeProviderFrameV3 {
    bytes: Vec<u8>,
    _lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl std::ops::Deref for NativeProviderFrameV3 {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}
fn read_frame_v3(stream: &mut impl Read) -> Result<NativeProviderFrameV3, String> {
    read_frame_capped_v4(stream, PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3)
}
fn read_frame_capped_v4(stream: &mut impl Read, cap: u64) -> Result<NativeProviderFrameV3, String> {
    let mut length = [0u8; 8];
    stream.read_exact(&mut length).map_err(|e| e.to_string())?;
    let length = u64::from_le_bytes(length);
    if length > cap || length > PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3 {
        return Err("native exchange frame exceeds its cap".into());
    }
    let lease = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(length).map_err(|e| e.to_string())?;
    let mut bytes = vec![0u8; length as usize];
    stream.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(NativeProviderFrameV3 { bytes, _lease: lease })
}

/// The authority each held bundle was admitted under, kept beside the store so a reopen re-verifies every bundle
/// (`DurableMonetaryMaterialProviderV1::open`'s recovery asks for it per claim). Written whole, fsynced and renamed, BEFORE the
/// bundle is published: an entry without a bundle is harmless, a bundle without its entry would refuse the reopen.
struct AuthorityIndexV3 {
    file: PathBuf,
    rows: BTreeMap<Hash64, AuthorityRecordV4>,
    _native_lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthorityRecordV4(PalwMaterialNativeAuthorityV1);
impl kaspa_consensus_core::palw_native_resource_v1::NativeHeapV1 for AuthorityRecordV4 {
    fn native_heap_v1(&self, _: &str) -> kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1 {
        let PalwMaterialNativeAuthorityV1 { execution_root: _, class_id: _, artifact_root: _, ladder: _, form: _ } = self.0;
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::zero()
    }
}
impl kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1 for AuthorityRecordV4 {
    fn scan_native_wire_v1<R: Read>(
        r: &mut kaspa_consensus_core::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1> {
        use kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1;
        r.step()?;
        let shape = Hash64::scan_native_wire_v1(r)?
            .merge(Hash64::scan_native_wire_v1(r)?)?
            .merge(Hash64::scan_native_wire_v1(r)?)?
            .merge(u64::scan_native_wire_v1(r)?)?;
        if r.byte()? > 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "authority prompt form unknown"));
        }
        Ok(shape)
    }
}

type AuthorityRowV3 = (Hash64, Hash64, Hash64, Hash64, u64, u8);
const AUTHORITY_ROW_BYTES_V3: usize = 4 * 64 + 8 + 1;
const AUTHORITY_MAX_ROWS_V3: usize = kaspa_consensus_core::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_MAX_CONTRACTS_V1 as usize
    * kaspa_consensus_core::palw_material_capacity_v1::PALW_MATERIAL_MAX_SCOPES_V1;
const AUTHORITY_MAX_BYTES_V3: u64 = (4 + AUTHORITY_MAX_ROWS_V3 * AUTHORITY_ROW_BYTES_V3) as u64;

impl AuthorityIndexV3 {
    fn projection_lease_v4(
        count: usize,
    ) -> Result<Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>, String> {
        use kaspa_consensus_core::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, reserve_native_requests_v1};
        if count > AUTHORITY_MAX_ROWS_V3 {
            return Err("authority typed row count exceeds cap".into());
        }
        let count32 = u32::try_from(count).map_err(|_| "authority row count overflow")?;
        let bytes = (count as u64)
            .checked_mul(AUTHORITY_ROW_BYTES_V3 as u64)
            .and_then(|n| n.checked_add(4))
            .ok_or("authority canonical extent overflow")?;
        let stream = std::io::Cursor::new(count32.to_le_bytes()).chain(std::io::repeat(0).take(bytes - 4));
        let mut reader = NativeWireReaderV1::new(stream, bytes);
        let map = BTreeMap::<Hash64, AuthorityRecordV4>::scan_native_wire_v1(&mut reader).map_err(|e| e.to_string())?;
        if reader.remaining() != 0 {
            return Err("authority typed map extent differs".into());
        }
        let tuple_bytes = (count as u64)
            .checked_mul(2 * std::mem::size_of::<AuthorityRowV3>() as u64)
            .ok_or("authority projection ownership overflow")?;
        let requests = map
            .heap
            .checked_add(tuple_bytes)
            .and_then(|n| bytes.checked_mul(2).and_then(|m| n.checked_add(m)))
            .and_then(|n| n.checked_add(std::mem::size_of::<BTreeMap<Hash64, AuthorityRecordV4>>() as u64))
            .ok_or("authority native ownership overflow")?;
        reserve_native_requests_v1(requests).map_err(|e| e.to_string())
    }
    fn open(file: &Path) -> Result<Self, String> {
        let (rows, native_lease) = match std::fs::File::open(file) {
            Ok(source) => {
                let actual = source.metadata().map_err(|e| e.to_string())?.len();
                if actual > AUTHORITY_MAX_BYTES_V3 {
                    return Err("native provider authority index exceeds cap".into());
                }
                let _raw = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(
                    actual.checked_add(1).and_then(|n| n.checked_mul(2)).ok_or("authority raw ownership overflow")?,
                )
                .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                source.take(actual + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                if bytes.len() as u64 != actual {
                    return Err("authority journal extent changed".into());
                }
                if bytes.len() as u64 > AUTHORITY_MAX_BYTES_V3 || bytes.len() < 4 {
                    return Err("native provider authority index byte bound".into());
                }
                let count = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
                // Every row is exactly four 64-byte digests + u64 + u8. Reject
                // a forged Vec count before Borsh allocates any rows.
                if count > AUTHORITY_MAX_ROWS_V3
                    || 4usize.checked_add(count.checked_mul(AUTHORITY_ROW_BYTES_V3).ok_or("authority count overflow")?)
                        != Some(bytes.len())
                {
                    return Err("native provider authority index extent/count differs".into());
                }
                let lease = Self::projection_lease_v4(count)?;
                let decoded =
                    borsh::from_slice::<Vec<AuthorityRowV3>>(&bytes).map_err(|e| format!("native provider authority index: {e}"))?;
                let mut rows = BTreeMap::new();
                let mut previous = None;
                for (claim, execution_root, class_id, artifact_root, ladder, form) in decoded {
                    if previous.is_some_and(|p| p >= claim)
                        || claim == Hash64::default()
                        || execution_root == Hash64::default()
                        || class_id == Hash64::default()
                        || artifact_root == Hash64::default()
                        || ladder == 0
                    {
                        return Err("native provider authority index is unordered, duplicated or incomplete".into());
                    }
                    previous = Some(claim);
                    let form = match form {
                        0 => PalwPromptIdsFormV1::Flat,
                        1 => PalwPromptIdsFormV1::MerkleV1,
                        _ => return Err("native provider authority index names an unknown prompt form".into()),
                    };
                    rows.insert(
                        claim,
                        AuthorityRecordV4(PalwMaterialNativeAuthorityV1 { execution_root, class_id, artifact_root, ladder, form }),
                    );
                }
                (rows, lease)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (BTreeMap::new(), None),
            Err(e) => return Err(e.to_string()),
        };
        Ok(Self { file: file.to_path_buf(), rows, _native_lease: native_lease })
    }
    fn get(&self, claim: &Hash64) -> Option<PalwMaterialNativeAuthorityV1> {
        self.rows.get(claim).map(|r| r.0)
    }
    fn record(&mut self, claim: Hash64, authority: PalwMaterialNativeAuthorityV1) -> Result<(), String> {
        match self.rows.get(&claim) {
            Some(known) if known.0 == authority => return Ok(()),
            Some(_) => return Err("a claim's native authority changed between two Ready requests".into()),
            None => {}
        }
        if self.rows.len() >= AUTHORITY_MAX_ROWS_V3 {
            return Err("native provider authority index capacity exhausted".into());
        }
        if claim == Hash64::default()
            || authority.execution_root == Hash64::default()
            || authority.class_id == Hash64::default()
            || authority.artifact_root == Hash64::default()
            || authority.ladder == 0
        {
            return Err("native provider authority identity incomplete".into());
        }
        let lease = Self::projection_lease_v4(self.rows.len() + 1)?;
        let mut rows = self.rows.clone();
        rows.insert(claim, AuthorityRecordV4(authority));
        self.publish(rows, lease)
    }
    /// Called only with claims of exact consents deleted under a true native
    /// finality witness, after subtracting claims any live store still retains.
    fn remove_finalized(&mut self, claims: &BTreeSet<Hash64>) -> Result<(), String> {
        let lease = Self::projection_lease_v4(self.rows.len())?;
        let mut rows = self.rows.clone();
        rows.retain(|claim, _| !claims.contains(claim));
        if rows.len() == self.rows.len() {
            return Ok(());
        }
        self.publish(rows, lease)
    }
    fn publish(
        &mut self,
        rows: BTreeMap<Hash64, AuthorityRecordV4>,
        lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
    ) -> Result<(), String> {
        if rows.len() > AUTHORITY_MAX_ROWS_V3 {
            return Err("authority publication exceeds cap".into());
        }
        let encoded: Vec<AuthorityRowV3> = rows
            .iter()
            .map(|(claim, record)| {
                let a = &record.0;
                let form = match a.form {
                    PalwPromptIdsFormV1::Flat => 0,
                    PalwPromptIdsFormV1::MerkleV1 => 1,
                };
                (*claim, a.execution_root, a.class_id, a.artifact_root, a.ladder, form)
            })
            .collect();
        let parent = self.file.parent().ok_or("authority index has no parent directory")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temporary = self.file.with_extension("authorities.partial");
        let result = (|| {
            let mut out = std::fs::File::create(&temporary)?;
            out.write_all(&borsh::to_vec(&encoded)?)?;
            out.sync_all()?;
            std::fs::rename(&temporary, &self.file)?;
            std::fs::File::open(parent)?.sync_all()
        })();
        if let Err(error) = result {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        self.rows = rows;
        self._native_lease = lease;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_private_options_are_loopback_by_default_and_need_two_distinct_bonds() {
        assert!(PrivateNativeCustodySpecV3::parse(None, None, None, None).unwrap().is_none());
        assert!(PrivateNativeCustodySpecV3::parse(None, None, None, Some("remote")).is_err(), "a network with no socket");
        assert!(PrivateNativeCustodySpecV3::parse(None, None, Some("10.0.0.1:7000"), None).is_err(), "not loopback");
        assert!(PrivateNativeCustodySpecV3::parse(None, None, Some("127.0.0.1:0"), None).is_err(), "no port");
        assert!(PrivateNativeCustodySpecV3::parse(None, Some("127.0.0.1:7000"), None, None).is_err(), "a listener with no keys");
        let spec = PrivateNativeCustodySpecV3::parse(None, None, Some("127.0.0.1:7000"), None).unwrap().unwrap();
        assert!(spec.providers.is_empty() && spec.listen.is_none());
        assert_eq!(spec.network, PrivateNativeNetworkV3::Loopback);
        let remote = PrivateNativeCustodySpecV3::parse(None, None, Some("10.0.0.1:7000"), Some("remote")).unwrap().unwrap();
        assert_eq!((remote.network, remote.exchange), (PrivateNativeNetworkV3::Remote, Some("10.0.0.1:7000".parse().unwrap())));
        assert!(PrivateNativeCustodySpecV3::parse(None, None, Some("0.0.0.0:7000"), Some("remote")).is_err(), "cannot dial 0.0.0.0");
        assert!(PrivateNativeCustodySpecV3::parse(None, None, Some("10.0.0.1:7000"), Some("lan")).is_err(), "unknown network");
        let dir = tempfile::tempdir().unwrap();
        let seed = |n: u8| {
            let path = dir.path().join(format!("bond-{n}.seed"));
            kaspa_pq_validator_core::write_validator_seed(path.to_str().unwrap(), &[n; 32]).unwrap();
            path.to_str().unwrap().to_owned()
        };
        let (a, b) = (seed(1), seed(2));
        let txid = Hash64::from_u64_word(9);
        let keys = format!("{a}@{txid}:1,{b}@{txid}:2");
        let spec = PrivateNativeCustodySpecV3::parse(Some(&keys), Some("127.0.0.1:7001"), None, None).unwrap().unwrap();
        assert_eq!(spec.providers.len(), 2);
        let remote = PrivateNativeCustodySpecV3::parse(Some(&keys), Some("0.0.0.0:7001"), None, Some("remote")).unwrap().unwrap();
        assert_eq!(remote.listen, Some("0.0.0.0:7001".parse().unwrap()), "a remote provider may listen on every interface");
        assert!(PrivateNativeCustodySpecV3::parse(Some(&keys), Some("0.0.0.0:7001"), None, None).is_err(), "loopback by default");
        assert!(PrivateNativeCustodySpecV3::parse(Some(&format!("{a}@{txid}:1")), None, None, None).is_err(), "one bond is no roster");
        assert!(
            PrivateNativeCustodySpecV3::parse(Some(&format!("{a}@{txid}:1,{a}@{txid}:2")), None, None, None).is_err(),
            "one key twice"
        );
        assert!(
            PrivateNativeCustodySpecV3::parse(Some(&keys), None, Some("127.0.0.1:7002"), None).is_err(),
            "in-process providers and a socket exchange are exclusive"
        );
        assert!(PrivateNativeCustodySpecV3::parse(Some(&keys), Some("127.0.0.1:7003"), Some("127.0.0.1:7003"), None).is_err());
    }

    #[test]
    fn a_frame_round_trips_and_an_oversized_length_is_refused() {
        let mut wire = Vec::new();
        write_frame_v3(&mut wire, b"ready").unwrap();
        assert_eq!(&*read_frame_v3(&mut wire.as_slice()).unwrap(), b"ready");
        let mut huge = (PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3 + 1).to_le_bytes().to_vec();
        huge.extend([0u8; 8]);
        assert!(read_frame_v3(&mut huge.as_slice()).is_err());
    }

    #[test]
    fn the_authority_index_survives_a_reopen_and_refuses_a_changed_authority() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p.authorities");
        let a = PalwMaterialNativeAuthorityV1 {
            execution_root: Hash64::from_u64_word(1),
            class_id: Hash64::from_u64_word(2),
            artifact_root: Hash64::from_u64_word(3),
            ladder: 1 << 22,
            form: PalwPromptIdsFormV1::MerkleV1,
        };
        let claim = Hash64::from_u64_word(4);
        AuthorityIndexV3::open(&file).unwrap().record(claim, a).unwrap();
        let mut reopened = AuthorityIndexV3::open(&file).unwrap();
        assert_eq!(reopened.get(&claim), Some(a));
        reopened.record(claim, a).unwrap();
        assert!(reopened.record(claim, PalwMaterialNativeAuthorityV1 { ladder: 1, ..a }).is_err());
    }
    #[test]
    fn authority_index_refuses_length_bombs_duplicates_and_cleans_only_completed_claims() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gc.authorities");
        let h = Hash64::from_u64_word;
        let authority = PalwMaterialNativeAuthorityV1 {
            execution_root: h(2),
            class_id: h(3),
            artifact_root: h(4),
            ladder: 64,
            form: PalwPromptIdsFormV1::MerkleV1,
        };
        let mut index = AuthorityIndexV3::open(&file).unwrap();
        index.record(h(10), authority).unwrap();
        index.record(h(11), authority).unwrap();
        index.remove_finalized(&std::iter::once(h(10)).collect()).unwrap();
        let reopened = AuthorityIndexV3::open(&file).unwrap();
        assert!(reopened.get(&h(10)).is_none());
        assert_eq!(reopened.get(&h(11)), Some(authority));
        let row = (h(11), authority.execution_root, authority.class_id, authority.artifact_root, authority.ladder, 1u8);
        std::fs::write(&file, borsh::to_vec(&vec![row, row]).unwrap()).unwrap();
        assert!(AuthorityIndexV3::open(&file).is_err());
        std::fs::write(&file, u32::MAX.to_le_bytes()).unwrap();
        assert!(AuthorityIndexV3::open(&file).is_err());
    }
}

#[path = "native_kernel_material_ingress_v4.rs"]
mod kernel_ingress_v4;
#[path = "native_recovery_runtime_v4.rs"]
mod recovery_runtime_v4;

#[path = "native_kernel_material_query_v4.rs"]
mod kernel_query_v4;

#[path = "native_kernel_material_replacement_v4.rs"]
mod kernel_replacement_v4;
