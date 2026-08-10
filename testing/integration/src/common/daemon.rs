use kaspa_consensus_core::network::NetworkId;
use kaspa_core::{core::Core, signals::Shutdown, task::runtime::AsyncRuntime};
use kaspa_database::utils::get_kaspa_tempdir;
use kaspa_grpc_client::GrpcClient;
use kaspa_grpc_server::service::GrpcService;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_utils::{networking::ContextualNetAddress, triggers::Listener};
use kaspad_lib::{args::Args, daemon::create_core_with_runtime};
use parking_lot::RwLock;
use std::{ops::Deref, sync::Arc, time::Duration};
use tempfile::TempDir;

use kaspa_grpc_client::ClientPool;

pub struct ClientManager {
    pub args: RwLock<Args>,

    /// Type and suffix of the daemon network
    pub network: NetworkId,

    /// Clients subscription context
    pub context: SubscriptionContext,

    // Daemon ports
    pub rpc_port: u16,
    pub p2p_port: u16,
}

impl ClientManager {
    pub fn new(args: Args) -> Self {
        let network = args.network();
        let context = SubscriptionContext::with_options(None);
        let rpc_port = args.rpclisten.unwrap().normalize(0).port;
        let p2p_port = args.listen.unwrap().normalize(0).port;
        let args = RwLock::new(args);
        Self { args, network, context, rpc_port, p2p_port }
    }

    pub async fn new_client(&self) -> GrpcClient {
        GrpcClient::connect_with_args(
            NotificationMode::Direct,
            format!("grpc://localhost:{}", self.rpc_port),
            Some(self.context.clone()),
            false,
            None,
            false,
            Some(500_000),
            Default::default(),
        )
        .await
        .unwrap()
    }

    pub async fn new_clients(&self, count: usize) -> Vec<GrpcClient> {
        let mut clients = Vec::with_capacity(count);
        for _ in 0..count {
            clients.push(self.new_client().await);
        }
        clients
    }

    pub async fn new_multi_listener_client(&self) -> GrpcClient {
        GrpcClient::connect_with_args(
            NotificationMode::MultiListeners,
            format!("grpc://localhost:{}", self.rpc_port),
            Some(self.context.clone()),
            true,
            None,
            false,
            Some(500_000),
            Default::default(),
        )
        .await
        .unwrap()
    }

    pub async fn new_client_pool<T: Send + 'static>(&self, pool_size: usize, distribution_channel_capacity: usize) -> ClientPool<T> {
        let mut clients = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            clients.push(Arc::new(self.new_client().await));
        }
        ClientPool::new(clients, distribution_channel_capacity)
    }
}

pub struct Daemon {
    client_manager: Arc<ClientManager>,

    pub core: Arc<Core>,
    grpc_server_started: Listener,
    shutdown_requested: Listener,
    workers: Option<Vec<std::thread::JoinHandle<()>>>,

    /// Shared so a restart can rebuild over the same data directory. The directory is removed when
    /// the last daemon holding it drops, not when the first one shuts down.
    appdir_tempdir: Arc<TempDir>,
}

/// A port no other daemon in this process will be given.
///
/// This used to draw at random and confirm the choice by binding and immediately dropping a
/// listener. Two problems, both of which a soak hits and a single test does not.
///
/// The draw can repeat. Over a run that starts many daemons, each taking four ports, two calls
/// returning the same number stops being unlikely — and both callers then see it bind, because the
/// first one dropped its listener before the second looked.
///
/// And the confirmation is not one. Between the drop and the daemon actually binding, seconds
/// later, anything may take the port. Observed:
///
/// ```text
/// thread 'tokio-runtime-worker' panicked at rpc/wrpc/server/src/service.rs:160:
/// WRPC Server bind error on 0.0.0.0:51215: Listen(... Address already in use (os error 48))
/// ```
///
/// That panic is in a spawned task, so the test carried on and reported success. In this codebase
/// a panic in a node worker thread is what a real defect looks like, so harness noise here costs
/// real triage time — this one was investigated as a possible product bug before being recognised.
///
/// Handing ports out from a process-wide counter removes the repeat entirely: no two calls, on any
/// thread, ever return the same number. The bind check stays, because it is still the only way to
/// notice a port already held by something outside this process — it just no longer carries the
/// weight it could not bear.
///
/// The counter starts somewhere random so two concurrent `cargo test` processes do not march in
/// step, and the test bind uses `0.0.0.0` to match what the daemon will actually ask for.
fn free_port() -> u16 {
    /// Above the ephemeral range on Linux and macOS, so the OS is not handing these out to
    /// outgoing connections while the fixture is handing them to daemons.
    const FLOOR: u16 = 20_000;
    const CEILING: u16 = 60_000;

    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    static BASE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    let base = *BASE.get_or_init(|| FLOOR + rand::random::<u16>() % (CEILING - FLOOR));

    for _ in 0..(CEILING - FLOOR) {
        let offset = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let port = FLOOR + (base - FLOOR).wrapping_add(offset) % (CEILING - FLOOR);
        // Bound to this port only if nothing outside the process holds it. The listener is dropped
        // immediately, as it must be — the daemon binds it later itself — so this narrows the
        // window rather than closing it. Closing it would mean passing bound sockets into the
        // daemon, which is a change to the node rather than to its fixture.
        if let Ok(listener) = std::net::TcpListener::bind(("0.0.0.0", port)) {
            drop(listener);
            return port;
        }
    }
    panic!("no free port in {FLOOR}..{CEILING} after a full sweep — something is holding the whole range");
}

fn port_from_address(addr: Option<ContextualNetAddress>) -> u16 {
    addr.and_then(|x| if x.has_port() { Some(x.normalize(0).port) } else { None }).unwrap_or_else(free_port)
}

impl Daemon {
    pub fn fill_args_with_random_ports(args: &mut Args) {
        let rpc_port = port_from_address(args.rpclisten);
        let p2p_port = port_from_address(args.listen);
        let rpc_json_port = free_port();
        let rpc_borsh_port = free_port();

        args.rpclisten = Some(format!("0.0.0.0:{rpc_port}").try_into().unwrap());
        args.listen = Some(format!("0.0.0.0:{p2p_port}").try_into().unwrap());
        args.rpclisten_json = Some(format!("0.0.0.0:{rpc_json_port}").parse().unwrap());
        args.rpclisten_borsh = Some(format!("0.0.0.0:{rpc_borsh_port}").parse().unwrap());
    }

    pub fn new_random(fd_total_budget: i32) -> Daemon {
        // UPnP registration might take some time and is not needed for usual daemon tests
        let args = Args { devnet: true, disable_upnp: true, ..Default::default() };
        Self::new_random_with_args(args, fd_total_budget)
    }

    pub fn new_random_with_args(mut args: Args, fd_total_budget: i32) -> Daemon {
        Self::fill_args_with_random_ports(&mut args);
        let client_manager = Arc::new(ClientManager::new(args));
        Self::with_manager(client_manager, fd_total_budget)
    }

    pub fn with_manager(client_manager: Arc<ClientManager>, fd_total_budget: i32) -> Daemon {
        Self::with_manager_in(client_manager, Arc::new(get_kaspa_tempdir()), fd_total_budget)
    }

    /// Build a daemon over a specific data directory. See [`Daemon::restarted`].
    pub fn with_manager_in(client_manager: Arc<ClientManager>, appdir_tempdir: Arc<TempDir>, fd_total_budget: i32) -> Daemon {
        client_manager.args.write().appdir = Some(appdir_tempdir.path().to_str().unwrap().to_owned());
        let (core, _) = create_core_with_runtime(&Default::default(), &client_manager.args.read(), fd_total_budget);
        let async_service = &Arc::downcast::<AsyncRuntime>(core.find(AsyncRuntime::IDENT).unwrap().into_any_arc()).unwrap();
        let rpc_core_service =
            &Arc::downcast::<RpcCoreService>(async_service.find(RpcCoreService::IDENT).unwrap().into_any_arc()).unwrap();
        let shutdown_requested = rpc_core_service.core_shutdown_request_listener();
        let grpc_server = &Arc::downcast::<GrpcService>(async_service.find(GrpcService::IDENT).unwrap().into_any_arc()).unwrap();
        let grpc_server_started = grpc_server.started();
        Daemon { client_manager, core, grpc_server_started, shutdown_requested, workers: None, appdir_tempdir }
    }

    pub fn client_manager(&self) -> Arc<ClientManager> {
        self.client_manager.clone()
    }

    pub fn grpc_server_started(&self) -> Listener {
        self.grpc_server_started.clone()
    }

    pub fn shutdown_requested(&self) -> Listener {
        self.shutdown_requested.clone()
    }

    pub fn run(&mut self) {
        self.workers = Some(self.core.start());
    }

    pub fn join(&mut self) {
        if let Some(workers) = self.workers.take() {
            self.core.join(workers);
        }
    }

    pub async fn start(&mut self) -> GrpcClient {
        self.run();
        // Wait for the node to initialize before connecting to RPC
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.new_client().await
    }

    pub fn shutdown(&mut self) {
        self.core.shutdown();
        self.join();
    }

    /// Stop this daemon and bring a new one up over the same data directory and the same ports.
    ///
    /// What an operator restart looks like from the node's point of view, and the only way to test
    /// that state which must survive one actually does. Anything held only in memory is gone; what
    /// comes back is whatever was written to disk.
    ///
    /// The returned daemon is not started — call `start()` on it, as after `new_random_with_args`.
    pub fn restarted(&mut self, fd_total_budget: i32) -> Daemon {
        self.shutdown();
        Daemon::with_manager_in(self.client_manager.clone(), self.appdir_tempdir.clone(), fd_total_budget)
    }
}

impl Deref for Daemon {
    type Target = ClientManager;

    fn deref(&self) -> &Self::Target {
        &self.client_manager
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_two_daemons_are_ever_offered_the_same_port() {
        // The property the counter exists for. The old draw-at-random could return the same port
        // to two callers, and both would see it bind — the first had already dropped its listener.
        // Four hundred ports is a hundred daemons' worth, well past where a random draw's
        // collisions became likely.
        let ports: Vec<u16> = (0..400).map(|_| free_port()).collect();
        let unique: HashSet<u16> = ports.iter().copied().collect();
        assert_eq!(unique.len(), ports.len(), "free_port() handed the same port out twice");
        assert!(ports.iter().all(|p| *p >= 20_000), "ports must sit above the ephemeral range");
    }

    #[test]
    fn ports_do_not_repeat_across_threads() {
        // Daemons are started from several threads in the integration tests, so the counter has to
        // be shared rather than thread-local — a per-thread sequence would collide immediately.
        let handles: Vec<_> = (0..4).map(|_| std::thread::spawn(|| (0..50).map(|_| free_port()).collect::<Vec<_>>())).collect();
        let all: Vec<u16> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        let unique: HashSet<u16> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "two threads were offered the same port");
    }
}
