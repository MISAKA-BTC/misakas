use crate::jsonrpc_event::JsonRpcEvent;
use crate::log_colors::LogColors;
use crate::net_utils::bind_addr_from_port;
use crate::stratum_context::StratumContext;
use hex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

/// Event handler function type
pub type EventHandler = Arc<
    dyn Fn(
            Arc<StratumContext>,
            JsonRpcEvent,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        + Send
        + Sync,
>;

/// Client listener trait
pub trait StratumClientListener: Send + Sync {
    fn on_connect(&self, ctx: Arc<StratumContext>);
    fn on_disconnect(&self, ctx: Arc<StratumContext>);
}

/// State generator function type
pub type StateGenerator = Box<dyn Fn() -> Arc<dyn std::any::Any + Send + Sync> + Send + Sync>;

/// Stratum listener statistics
#[derive(Debug, Default)]
pub struct StratumStats {
    pub disconnects: u64,
}

/// Configuration for the Stratum listener
pub struct StratumListenerConfig {
    pub handler_map: Arc<HashMap<String, EventHandler>>,
    pub on_connect: Arc<dyn Fn(Arc<StratumContext>) + Send + Sync>,
    pub on_disconnect: Arc<dyn Fn(Arc<StratumContext>) + Send + Sync>,
    pub port: String,
    /// Max concurrent connections accepted (global resource-exhaustion guard). 0 = unlimited.
    pub max_connections: usize,
    /// Max concurrent connections from a single source IP. 0 = unlimited.
    pub max_connections_per_ip: usize,
}

/// Maximum bytes buffered for a single (newline-terminated) Stratum JSON-RPC message. A valid line is
/// far smaller; this caps memory for a client that never sends a newline.
const MAX_LINE_BYTES: usize = 16 * 1024;

/// Number of consecutive read timeouts (~5s each) a not-yet-handshaked client may accrue before it is
/// disconnected. Bounds pre-auth connection-holding without affecting authenticated miners.
const PRE_AUTH_IDLE_STRIKES: u32 = 6;

/// Absolute wall-clock deadline by which a connection must authorize, regardless of activity. Closes
/// the slow-trickle slot-hold (a client dripping bytes resets the idle-strike counter but cannot beat
/// this hard deadline). Generous enough for any real miner handshake.
const PRE_AUTH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// Decrements a per-IP connection counter when dropped (held for the lifetime of a client task).
struct PerIpGuard {
    ip: std::net::IpAddr,
    counts: Arc<parking_lot::Mutex<HashMap<std::net::IpAddr, u32>>>,
}

impl Drop for PerIpGuard {
    fn drop(&mut self) {
        let mut map = self.counts.lock();
        if let Some(c) = map.get_mut(&self.ip) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                map.remove(&self.ip);
            }
        }
    }
}

/// Stratum TCP listener
pub struct StratumListener {
    config: StratumListenerConfig,
    stats: Arc<parking_lot::Mutex<StratumStats>>,
    shutting_down: Arc<std::sync::atomic::AtomicBool>,
}

impl StratumListener {
    /// Create a new Stratum listener
    pub fn new(config: StratumListenerConfig) -> Self {
        Self {
            config,
            stats: Arc::new(parking_lot::Mutex::new(StratumStats::default())),
            shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start listening for connections
    pub async fn listen(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.listen_impl(None).await
    }

    pub async fn listen_with_shutdown(
        &self,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.listen_impl(Some(shutdown_rx)).await
    }

    async fn listen_impl(
        &self,
        mut shutdown_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.shutting_down.store(false, std::sync::atomic::Ordering::Release);

        // Ensure we bind to IPv4 (0.0.0.0) when given a bare port like ":5555" / "5555".
        let addr_str = bind_addr_from_port(&self.config.port);

        let listener =
            TcpListener::bind(&addr_str).await.map_err(|e| format!("failed listening to socket {}: {}", self.config.port, e))?;

        debug!("Stratum listener started on {}", self.config.port);

        let (disconnect_tx, mut disconnect_rx) = mpsc::unbounded_channel::<Arc<StratumContext>>();
        let disconnect_tx_clone = disconnect_tx.clone();
        let on_disconnect = Arc::clone(&self.config.on_disconnect);
        let stats = self.stats.clone();

        let mut disconnect_shutdown_rx = shutdown_rx.clone();
        tokio::spawn(async move {
            loop {
                if let Some(ref mut rx) = disconnect_shutdown_rx {
                    tokio::select! {
                        _ = rx.changed() => {
                            if *rx.borrow() {
                                break;
                            }
                        }
                        maybe_ctx = disconnect_rx.recv() => {
                            let Some(ctx) = maybe_ctx else {
                                break;
                            };
                            info!("[CONNECTION] client disconnecting - {}", ctx.remote_addr);
                            info!("[CONNECTION] Disconnect event for {}:{}", ctx.remote_addr, ctx.remote_port);
                            stats.lock().disconnects += 1;
                            on_disconnect(ctx);
                        }
                    }
                } else {
                    let Some(ctx) = disconnect_rx.recv().await else {
                        break;
                    };
                    info!("[CONNECTION] client disconnecting - {}", ctx.remote_addr);
                    info!("[CONNECTION] Disconnect event for {}:{}", ctx.remote_addr, ctx.remote_port);
                    stats.lock().disconnects += 1;
                    on_disconnect(ctx);
                }
            }
        });

        // Connection-rate limits. A bare port binds 0.0.0.0 (miners connect from the network), so the
        // accept path is unauthenticated and remote-reachable: cap concurrent connections globally and
        // per source IP to bound socket/task/memory exhaustion. 0 disables a cap.
        let global_cap =
            if self.config.max_connections == 0 { tokio::sync::Semaphore::MAX_PERMITS } else { self.config.max_connections };
        let conn_sem = Arc::new(tokio::sync::Semaphore::new(global_cap));
        let per_ip: Arc<parking_lot::Mutex<HashMap<std::net::IpAddr, u32>>> = Arc::new(parking_lot::Mutex::new(HashMap::new()));

        loop {
            if let Some(ref mut rx) = shutdown_rx {
                tokio::select! {
                    _ = rx.changed() => {
                        if *rx.borrow() {
                            self.shutting_down.store(true, std::sync::atomic::Ordering::Release);
                            break;
                        }
                    }
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                self.handle_new_connection(stream, addr, &conn_sem, &per_ip, &disconnect_tx_clone);
                            }
                            Err(e) => {
                            if self.shutting_down.load(std::sync::atomic::Ordering::Acquire) {
                                info!("stopping listening due to server shutdown");
                                break;
                            }
                            error!("[CONNECTION] ===== FAILED TO ACCEPT INCOMING CONNECTION =====");
                            error!("[CONNECTION] Error: {}", e);
                            error!("[CONNECTION] Error kind: {:?}", e.kind());
                            error!("[CONNECTION] Failed to accept connection: {} (kind: {:?})", e, e.kind());
                            }
                        }
                    }
                }
            } else {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        self.handle_new_connection(stream, addr, &conn_sem, &per_ip, &disconnect_tx_clone);
                    }
                    Err(e) => {
                        if self.shutting_down.load(std::sync::atomic::Ordering::Acquire) {
                            info!("stopping listening due to server shutdown");
                            break;
                        }
                        error!("[CONNECTION] ===== FAILED TO ACCEPT INCOMING CONNECTION =====");
                        error!("[CONNECTION] Error: {}", e);
                        error!("[CONNECTION] Error kind: {:?}", e.kind());
                        error!("[CONNECTION] Failed to accept connection: {} (kind: {:?})", e, e.kind());
                    }
                }
            }
        }

        Ok(())
    }

    /// Set up a freshly-accepted connection, enforcing the global and per-IP connection caps. On
    /// success spawns the per-client listener task (which owns the semaphore permit and per-IP guard
    /// for its lifetime, releasing both on disconnect). On cap exhaustion the stream is dropped, which
    /// closes the connection.
    fn handle_new_connection(
        &self,
        stream: tokio::net::TcpStream,
        addr: std::net::SocketAddr,
        conn_sem: &Arc<tokio::sync::Semaphore>,
        per_ip: &Arc<parking_lot::Mutex<HashMap<std::net::IpAddr, u32>>>,
        disconnect_tx: &mpsc::UnboundedSender<Arc<StratumContext>>,
    ) {
        let ip = addr.ip();

        // Global cap: owned permit, released when the client task ends.
        let permit = match Arc::clone(conn_sem).try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                warn!("[CONNECTION] global connection limit reached; rejecting {}", addr);
                return; // stream dropped -> connection closed
            }
        };

        // Per-IP cap.
        let ip_guard = {
            let mut map = per_ip.lock();
            let count = map.entry(ip).or_insert(0);
            if self.config.max_connections_per_ip != 0 && *count >= self.config.max_connections_per_ip as u32 {
                warn!("[CONNECTION] per-IP connection limit ({}) reached for {}; rejecting", self.config.max_connections_per_ip, ip);
                return; // permit + stream dropped -> released / closed
            }
            *count += 1;
            PerIpGuard { ip, counts: Arc::clone(per_ip) }
        };

        let remote_addr = ip.to_string();
        let remote_port = addr.port();
        debug!("[CONNECTION] new client connecting - {}:{}", remote_addr, remote_port);

        let state = Arc::new(crate::mining_state::MiningState::new());
        let ctx = StratumContext::new(remote_addr, remote_port, stream, state, disconnect_tx.clone());
        (self.config.on_connect)(ctx.clone());

        let ctx_clone = ctx.clone();
        let handler_map = self.config.handler_map.clone();
        tokio::spawn(async move {
            // Hold the permit and per-IP guard for the lifetime of the connection.
            let _permit = permit;
            let _ip_guard = ip_guard;
            debug!("[CONNECTION] Client listener task started for {}:{}", ctx_clone.remote_addr, ctx_clone.remote_port);
            Self::spawn_client_listener(ctx_clone, &handler_map).await;
            debug!("[CONNECTION] Client listener task ended");
        });
    }

    /// Spawn a client listener task
    async fn spawn_client_listener(ctx: Arc<StratumContext>, handler_map: &Arc<HashMap<String, EventHandler>>) {
        debug!("[CLIENT_LISTENER] Starting client listener for {}:{}", ctx.remote_addr, ctx.remote_port);
        let mut buffer = [0u8; 1024];
        let mut line_buffer = String::new();
        let mut first_message = true;
        // Consecutive read-timeout counter, used to reap idle pre-handshake clients (F3).
        let mut idle_strikes = 0u32;
        // Absolute wall-clock deadline by which a connection must AUTHORIZE. The idle-strike counter
        // resets on any byte, so a slow-trickle client (1 byte every few seconds) could otherwise
        // hold a connection slot indefinitely without authorizing; this hard deadline closes that.
        let connected_at = tokio::time::Instant::now();

        loop {
            // Check if disconnected
            if !ctx.connected() {
                debug!("[CLIENT_LISTENER] Client {}:{} disconnected", ctx.remote_addr, ctx.remote_port);
                break;
            }

            // Hard pre-auth deadline: drop any connection that has not authorized within the window,
            // regardless of how much it has trickled (closes the slow-trickle slot-hold).
            if ctx.wallet_addr.lock().is_empty() && connected_at.elapsed() > PRE_AUTH_DEADLINE {
                warn!(
                    "[CONNECTION] {}:{} did not authorize within {}s; disconnecting (pre-auth deadline)",
                    ctx.remote_addr,
                    ctx.remote_port,
                    PRE_AUTH_DEADLINE.as_secs()
                );
                ctx.disconnect();
                break;
            }

            // Get read half for reading (must drop guard before await)
            let read_half_opt = {
                let mut read_guard = ctx.get_read_half();
                read_guard.take()
            };

            let read_result = if let Some(mut read_half) = read_half_opt {
                // Set read deadline
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

                let result = tokio::time::timeout_at(deadline, read_half.read(&mut buffer)).await;

                // Put read half back
                {
                    let mut read_guard = ctx.get_read_half();
                    *read_guard = Some(read_half);
                }

                result
            } else {
                // Read half is None, disconnect
                warn!("[CONNECTION] Read half is None for {}, disconnecting", ctx.remote_addr);
                break;
            };

            match read_result {
                Ok(Ok(0)) => {
                    // EOF - client closed connection
                    let worker_name = ctx.worker_name.lock().clone();
                    let remote_app = ctx.remote_app.lock().clone();
                    let pending_buffer_bytes = line_buffer.len();
                    let is_pre_handshake = worker_name.is_empty() && remote_app.is_empty();
                    if is_pre_handshake && first_message && pending_buffer_bytes == 0 {
                        debug!(
                            "[CONNECTION] Client {}:{} closed connection (EOF) worker='{}' app='{}' first_message={} pending_buffer_bytes={}",
                            ctx.remote_addr, ctx.remote_port, worker_name, remote_app, first_message, pending_buffer_bytes
                        );
                    } else {
                        info!(
                            "[CONNECTION] Client {}:{} closed connection (EOF) worker='{}' app='{}' first_message={} pending_buffer_bytes={}",
                            ctx.remote_addr, ctx.remote_port, worker_name, remote_app, first_message, pending_buffer_bytes
                        );
                    }
                    break;
                }
                Ok(Ok(n)) => {
                    debug!("[CLIENT_LISTENER] Read {} bytes from {}:{}", n, ctx.remote_addr, ctx.remote_port);
                    idle_strikes = 0; // client is active

                    // Remove null bytes and process
                    let data: Vec<u8> = buffer[..n].iter().copied().filter(|&b| b != 0).collect();

                    if first_message {
                        let wallet_addr = ctx.wallet_addr.lock().clone();
                        let worker_name = ctx.worker_name.lock().clone();
                        let remote_app = ctx.remote_app.lock().clone();
                        let message_str = String::from_utf8_lossy(&data);

                        // Check for HTTP/2/gRPC protocol in first message (before logging)
                        let first_line = message_str.lines().next().unwrap_or("").trim();
                        if first_line.starts_with("PRI * HTTP/2.0")
                            || first_line.starts_with("PRI * HTTP/2")
                            || first_line == "SM"
                            || first_line.starts_with("GET ")
                            || first_line.starts_with("POST ")
                            || first_line.starts_with("PUT ")
                            || first_line.starts_with("DELETE ")
                            || first_line.starts_with("HEAD ")
                            || first_line.starts_with("OPTIONS ")
                        {
                            error!("{}", LogColors::error("========================================"));
                            error!("{}", LogColors::error("===== PROTOCOL MISMATCH DETECTED (FIRST MESSAGE) ===== "));
                            error!("{}", LogColors::error("========================================"));
                            error!("{} {}", LogColors::error("[ERROR]"), LogColors::label("Client Information:"));
                            error!(
                                "{} {} {}",
                                LogColors::error("[ERROR]"),
                                LogColors::label("  - IP Address:"),
                                format!("{}:{}", ctx.remote_addr, ctx.remote_port)
                            );
                            error!(
                                "{} {} {}",
                                LogColors::error("[ERROR]"),
                                LogColors::label("  - Protocol Detected:"),
                                "HTTP/2 or HTTP (gRPC)"
                            );
                            error!(
                                "{} {} {}",
                                LogColors::error("[ERROR]"),
                                LogColors::label("  - Expected Protocol:"),
                                "Plain TCP/JSON-RPC (Stratum)"
                            );
                            error!(
                                "{} {} {}",
                                LogColors::error("[ERROR]"),
                                LogColors::label("  - First Message (hex):"),
                                hex::encode(&data)
                            );
                            error!(
                                "{} {} {}",
                                LogColors::error("[ERROR]"),
                                LogColors::label("  - First Message (string):"),
                                first_line
                            );
                            error!("{} {}", LogColors::error("[ERROR]"), LogColors::label("Action:"));
                            error!(
                                "{} {}",
                                LogColors::error("[ERROR]"),
                                "  * Rejecting connection - Stratum port only accepts JSON-RPC over plain TCP"
                            );
                            error!(
                                "{} {}",
                                LogColors::error("[ERROR]"),
                                "  * HTTP/2/gRPC connections should use the Kaspa node port (16110), not the bridge port (5555)"
                            );
                            error!("{} {}", LogColors::error("[ERROR]"), "  * Closing connection immediately");
                            error!("{}", LogColors::error("========================================"));

                            // Close connection
                            ctx.disconnect();
                            break;
                        }

                        debug!("{}", LogColors::asic_to_bridge("========================================"));
                        debug!("{}", LogColors::asic_to_bridge("===== FIRST MESSAGE FROM ASIC ===== "));
                        debug!("{}", LogColors::asic_to_bridge("========================================"));
                        debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Connection Information:"));
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - IP Address:"),
                            format!("{}:{}", ctx.remote_addr, ctx.remote_port)
                        );
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - Wallet Address:"),
                            format!("'{}'", wallet_addr)
                        );
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - Worker Name:"),
                            format!("'{}'", worker_name)
                        );
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - Miner Application:"),
                            format!("'{}'", remote_app)
                        );
                        debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("First Message Data:"));
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - Raw Bytes (hex):"),
                            hex::encode(&data)
                        );
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - Raw Bytes Length:"),
                            format!("{} bytes", data.len())
                        );
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - Message as String:"),
                            message_str
                        );
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - String Length:"),
                            format!("{} characters", message_str.len())
                        );
                        debug!(
                            "{} {} {}",
                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                            LogColors::label("  - String Length:"),
                            format!("{} bytes (UTF-8)", message_str.len())
                        );
                        // Show byte-by-byte breakdown for first 100 bytes
                        if data.len() <= 100 {
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - Byte Breakdown:"),
                                format!("{:?}", data)
                            );
                        } else {
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - First 100 Bytes:"),
                                format!("{:?}", &data[..100.min(data.len())])
                            );
                        }
                        debug!("{}", LogColors::asic_to_bridge("========================================"));
                        first_message = false;
                    }

                    line_buffer.push_str(&String::from_utf8_lossy(&data));

                    // Cap the pending buffer: if it has grown past the limit without yet containing a
                    // complete line, the client is sending data with no newline (memory-DoS). Drop it.
                    if line_buffer.len() > MAX_LINE_BYTES && !line_buffer.contains('\n') {
                        warn!(
                            "[CONNECTION] {}:{} exceeded max line length ({} > {} bytes) with no newline; disconnecting",
                            ctx.remote_addr,
                            ctx.remote_port,
                            line_buffer.len(),
                            MAX_LINE_BYTES
                        );
                        ctx.disconnect();
                        break;
                    }

                    // Process complete lines
                    while let Some(newline_pos) = line_buffer.find('\n') {
                        let line = line_buffer[..newline_pos].trim().to_string();
                        line_buffer = line_buffer[newline_pos + 1..].to_string();

                        if !line.is_empty() {
                            // Get client context for detailed logging
                            let wallet_addr = ctx.wallet_addr.lock().clone();
                            let worker_name = ctx.worker_name.lock().clone();
                            let remote_app = ctx.remote_app.lock().clone();

                            // Detect HTTP/2/gRPC connections early and reject them
                            // HTTP/2 connection preface starts with "PRI * HTTP/2.0"
                            if line.starts_with("PRI * HTTP/2.0")
                                || line.starts_with("PRI * HTTP/2")
                                || line == "SM"
                                || line.starts_with("GET ")
                                || line.starts_with("POST ")
                                || line.starts_with("PUT ")
                                || line.starts_with("DELETE ")
                                || line.starts_with("HEAD ")
                                || line.starts_with("OPTIONS ")
                            {
                                error!("{}", LogColors::error("========================================"));
                                error!("{}", LogColors::error("===== PROTOCOL MISMATCH DETECTED ===== "));
                                error!("{}", LogColors::error("========================================"));
                                error!("{} {}", LogColors::error("[ERROR]"), LogColors::label("Client Information:"));
                                error!(
                                    "{} {} {}",
                                    LogColors::error("[ERROR]"),
                                    LogColors::label("  - IP Address:"),
                                    format!("{}:{}", ctx.remote_addr, ctx.remote_port)
                                );
                                error!(
                                    "{} {} {}",
                                    LogColors::error("[ERROR]"),
                                    LogColors::label("  - Protocol Detected:"),
                                    "HTTP/2 or HTTP (gRPC)"
                                );
                                error!(
                                    "{} {} {}",
                                    LogColors::error("[ERROR]"),
                                    LogColors::label("  - Expected Protocol:"),
                                    "Plain TCP/JSON-RPC (Stratum)"
                                );
                                error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - Received Message:"), &line);
                                error!("{} {}", LogColors::error("[ERROR]"), LogColors::label("Action:"));
                                error!(
                                    "{} {}",
                                    LogColors::error("[ERROR]"),
                                    "  * Rejecting connection - Stratum port only accepts JSON-RPC over plain TCP"
                                );
                                error!(
                                    "{} {}",
                                    LogColors::error("[ERROR]"),
                                    "  * HTTP/2/gRPC connections should use the Kaspa node port (16110), not the bridge port (5555)"
                                );
                                error!("{} {}", LogColors::error("[ERROR]"), "  * Closing connection immediately");
                                error!("{}", LogColors::error("========================================"));

                                // Close connection
                                ctx.disconnect();
                                break;
                            }

                            // Log raw incoming message from ASIC at DEBUG level (verbose details)
                            debug!("{}", LogColors::asic_to_bridge("========================================"));
                            debug!("{}", LogColors::asic_to_bridge("===== RECEIVED MESSAGE FROM ASIC ===== "));
                            debug!("{}", LogColors::asic_to_bridge("========================================"));
                            debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Client Information:"));
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - IP Address:"),
                                format!("{}:{}", ctx.remote_addr, ctx.remote_port)
                            );
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - Wallet Address:"),
                                format!("'{}'", wallet_addr)
                            );
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - Worker Name:"),
                                format!("'{}'", worker_name)
                            );
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - Miner Application:"),
                                format!("'{}'", remote_app)
                            );
                            debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Raw Message Data:"));
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - Raw Message:"),
                                line
                            );
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - Message Length:"),
                                format!("{} bytes", line.len())
                            );
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - Message Length:"),
                                format!("{} characters", line.chars().count())
                            );
                            debug!(
                                "{} {} {}",
                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                LogColors::label("  - Raw Bytes (hex):"),
                                hex::encode(line.as_bytes())
                            );

                            match crate::jsonrpc_event::unmarshal_event(&line) {
                                Ok(event) => {
                                    let params_str = serde_json::to_string(&event.params).unwrap_or_else(|_| "[]".to_string());

                                    // Log parsed event details at DEBUG level (detailed logs moved to debug)
                                    debug!("{}", LogColors::asic_to_bridge("===== PARSING SUCCESSFUL ===== "));
                                    debug!(
                                        "{} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("Parsed Event Structure:")
                                    );
                                    debug!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Method:"),
                                        format!("'{}'", event.method)
                                    );
                                    debug!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Event ID:"),
                                        format!("{:?}", event.id)
                                    );
                                    debug!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - JSON-RPC Version:"),
                                        format!("'{}'", event.jsonrpc)
                                    );
                                    debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Parameters:"));
                                    debug!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Params Count:"),
                                        event.params.len()
                                    );
                                    debug!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Params JSON:"),
                                        params_str
                                    );
                                    debug!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Params Length:"),
                                        format!("{} characters", params_str.len())
                                    );
                                    // Log each param individually with type information
                                    for (idx, param) in event.params.iter().enumerate() {
                                        let param_str = serde_json::to_string(param).unwrap_or_else(|_| "N/A".to_string());
                                        let param_type = if param.is_string() {
                                            let s = param.as_str().unwrap_or("");
                                            format!("String (length: {}, value: '{}')", s.len(), s)
                                        } else if param.is_number() {
                                            format!("Number (value: {})", param)
                                        } else if param.is_array() {
                                            let arr = param.as_array().unwrap();
                                            format!(
                                                "Array (length: {}, items: {:?})",
                                                arr.len(),
                                                arr.iter()
                                                    .take(5)
                                                    .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "?".to_string()))
                                                    .collect::<Vec<_>>()
                                            )
                                        } else if param.is_object() {
                                            "Object".to_string()
                                        } else if param.is_boolean() {
                                            format!("Boolean (value: {})", param.as_bool().unwrap_or(false))
                                        } else {
                                            "Null".to_string()
                                        };
                                        debug!(
                                            "{} {} {}",
                                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                            LogColors::label(&format!("  - Param[{}]:", idx)),
                                            format!("{} (type: {})", param_str, param_type)
                                        );
                                    }

                                    if let Some(handler) = handler_map.get(&event.method) {
                                        debug!("{}", LogColors::asic_to_bridge("===== PROCESSING MESSAGE ===== "));
                                        debug!(
                                            "{} {} {}",
                                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                            LogColors::label("  - Handler Found:"),
                                            "YES"
                                        );
                                        debug!(
                                            "{} {} {}",
                                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                            LogColors::label("  - Method:"),
                                            format!("'{}'", event.method)
                                        );
                                        debug!(
                                            "{} {}",
                                            LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                            "  - Starting handler execution..."
                                        );
                                        if let Err(e) = handler(ctx.clone(), event).await {
                                            let error_msg = e.to_string();
                                            if error_msg.contains("stale") || error_msg.contains("job does not exist") {
                                                // Log stale job errors as debug (expected behavior, not important)
                                                debug!("{}", LogColors::asic_to_bridge("===== HANDLER EXECUTION RESULT ===== "));
                                                debug!(
                                                    "{} {} {}",
                                                    LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                                    LogColors::validation("  - Result:"),
                                                    "STALE JOB (expected - job no longer exists)"
                                                );
                                                debug!(
                                                    "{} {} {}",
                                                    LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                                    LogColors::label("  - Error Message:"),
                                                    error_msg
                                                );
                                            } else if error_msg.contains("job id is not parsable") {
                                                // Log parsing errors as warnings
                                                warn!(
                                                    "{} {} {}",
                                                    LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                                    LogColors::error("  - Result:"),
                                                    "ERROR (job ID parsing failed)"
                                                );
                                                warn!(
                                                    "{} {} {}",
                                                    LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                                    LogColors::label("  - Error Message:"),
                                                    error_msg
                                                );
                                            } else {
                                                error!(
                                                    "{} {} {}",
                                                    LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                                    LogColors::error("  - Result:"),
                                                    "ERROR (handler execution failed)"
                                                );
                                                error!(
                                                    "{} {} {}",
                                                    LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                                    LogColors::label("  - Error Message:"),
                                                    error_msg
                                                );
                                            }
                                        } else {
                                            debug!("{}", LogColors::asic_to_bridge("===== HANDLER EXECUTION RESULT ===== "));
                                            debug!(
                                                "{} {} {}",
                                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                                LogColors::label("  - Result:"),
                                                "SUCCESS"
                                            );
                                            debug!(
                                                "{} {}",
                                                LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                                "  - Message processed successfully"
                                            );
                                        }
                                        debug!("{}", LogColors::asic_to_bridge("========================================"));
                                    }
                                }
                                Err(e) => {
                                    error!("{}", LogColors::asic_to_bridge("========================================"));
                                    error!("{}", LogColors::error("===== ERROR PARSING MESSAGE ===== "));
                                    error!("{}", LogColors::asic_to_bridge("========================================"));
                                    error!(
                                        "{} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("Client Information:")
                                    );
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - IP Address:"),
                                        format!("{}:{}", ctx.remote_addr, ctx.remote_port)
                                    );
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Wallet Address:"),
                                        format!("'{}'", wallet_addr)
                                    );
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Worker Name:"),
                                        format!("'{}'", worker_name)
                                    );
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Miner Application:"),
                                        format!("'{}'", remote_app)
                                    );
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Failed Message:"));
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Raw Message:"),
                                        line
                                    );
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Message Length:"),
                                        format!("{} bytes", line.len())
                                    );
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Raw Bytes (hex):"),
                                        hex::encode(line.as_bytes())
                                    );
                                    error!(
                                        "{} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("Parse Error Details:")
                                    );
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Error Type:"),
                                        "JSON Parsing Failed"
                                    );
                                    error!(
                                        "{} {} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::error("  - Error Message:"),
                                        e
                                    );
                                    error!(
                                        "{} {}",
                                        LogColors::asic_to_bridge("[ASIC->BRIDGE]"),
                                        LogColors::label("  - Possible Causes:")
                                    );
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "    * Malformed JSON syntax");
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "    * Protocol mismatch");
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "    * Incomplete message");
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "    * Encoding issue");
                                    error!("{}", LogColors::asic_to_bridge("========================================"));
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    // Check if it's a connection closed error (expected when client disconnects)
                    let error_msg = e.to_string();
                    if error_msg.contains("forcibly closed")
                        || error_msg.contains("Connection reset")
                        || error_msg.contains("Broken pipe")
                        || e.kind() == std::io::ErrorKind::ConnectionReset
                        || e.kind() == std::io::ErrorKind::BrokenPipe
                    {
                        let worker_name = ctx.worker_name.lock().clone();
                        let remote_app = ctx.remote_app.lock().clone();
                        let is_pre_handshake = worker_name.is_empty() && remote_app.is_empty();
                        if is_pre_handshake {
                            debug!(
                                "[CONNECTION] Client {}:{} disconnected (reset/broken pipe) kind={:?} worker='{}' app='{}' msg='{}'",
                                ctx.remote_addr,
                                ctx.remote_port,
                                e.kind(),
                                worker_name,
                                remote_app,
                                error_msg
                            );
                        } else {
                            info!(
                                "[CONNECTION] Client {}:{} disconnected (reset/broken pipe) kind={:?} worker='{}' app='{}' msg='{}'",
                                ctx.remote_addr,
                                ctx.remote_port,
                                e.kind(),
                                worker_name,
                                remote_app,
                                error_msg
                            );
                        }
                    } else {
                        error!("error reading from socket: {}", e);
                    }
                    break;
                }
                Err(_) => {
                    // Read timeout. Reap clients that connect but never AUTHORIZE, so an unauthorized
                    // client cannot hold a connection (and its slot) open indefinitely. We gate on
                    // authorization (a non-empty wallet address, set only by mining.authorize) rather
                    // than on remote_app: a client can set remote_app via mining.subscribe and then go
                    // idle, which would otherwise escape a subscribe-only "pre_handshake" check.
                    idle_strikes = idle_strikes.saturating_add(1);
                    let is_unauthorized = ctx.wallet_addr.lock().is_empty();
                    if is_unauthorized && idle_strikes >= PRE_AUTH_IDLE_STRIKES {
                        warn!(
                            "[CONNECTION] {}:{} idle without authorizing (~{}s); disconnecting",
                            ctx.remote_addr,
                            ctx.remote_port,
                            idle_strikes * 5
                        );
                        ctx.disconnect();
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    continue;
                }
            }
        }

        ctx.disconnect();
    }

    /// Handle an event
    pub fn handle_event(
        &self,
        _ctx: Arc<StratumContext>,
        event: JsonRpcEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(_handler) = self.config.handler_map.get(&event.method) {
            // Note: This is a sync wrapper - actual handlers should be async
            // For now, we'll handle this in spawn_client_listener
            Ok(())
        } else {
            Ok(())
        }
    }
}
