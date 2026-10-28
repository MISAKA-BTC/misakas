/// Shared utilities for normalizing ports/bind addresses.
///
/// We intentionally keep the logic here minimal and dependency-free so both the library
/// (web/prom servers) and the binary (CLI/config parsing) can use the same behavior.
///
/// Supported input examples:
/// - ":3030"          -> ":3030"
/// - "3030"           -> ":3030"
/// - "127.0.0.1:3030" -> "127.0.0.1:3030"
/// - "0.0.0.0:3030"   -> "0.0.0.0:3030"
pub fn normalize_port(port_or_addr: &str) -> String {
    let s = port_or_addr.trim();
    if s.is_empty() {
        return String::new();
    }
    if s.starts_with(':') {
        s.to_string()
    } else if s.chars().all(|c| c.is_ascii_digit()) {
        format!(":{}", s)
    } else {
        s.to_string()
    }
}

/// Convert a port-or-address string into a concrete bind address suitable for `SocketAddr::parse()`.
///
/// A bare port (`":3030"` / `"3030"`) becomes `"0.0.0.0:3030"` (all interfaces). This is used for
/// the Stratum listener, which must accept miners from the network.
pub fn bind_addr_from_port(port_or_addr: &str) -> String {
    bind_addr_with_default_host(port_or_addr, "0.0.0.0")
}

/// Like [`bind_addr_from_port`] but a bare port defaults to loopback (`127.0.0.1`) instead of all
/// interfaces.
///
/// Used for management / monitoring endpoints (the web dashboard and the Prometheus server) so they
/// are **not** exposed to the network unless the operator opts in by writing an explicit address such
/// as `"0.0.0.0:3030"` or `"192.168.1.10:3030"` in the config. This is a fail-safe default: those
/// endpoints leak config/topology (`/api/config`, `/api/stats`) and, when writes are enabled, allow
/// `kaspad_address` to be repointed — they should never be world-reachable by accident.
pub fn bind_addr_from_port_local(port_or_addr: &str) -> String {
    bind_addr_with_default_host(port_or_addr, "127.0.0.1")
}

/// Shared helper: normalize a port-or-address and, if only a bare port was given, prepend
/// `default_host`. A fully-qualified `host:port` is passed through unchanged.
fn bind_addr_with_default_host(port_or_addr: &str, default_host: &str) -> String {
    let s = normalize_port(port_or_addr);
    if s.is_empty() {
        return s;
    }
    if s.starts_with(':') { format!("{}{}", default_host, s) } else { s }
}
