//! **`misaka dashboard`** — ADR-0122 Decision 9: the same answers as `status`, `work show`, `logs`
//! and `doctor`, on one page at `http://127.0.0.1:8791`.
//!
//! Not 8790: that is the gateway's OpenAI-compatible endpoint, which clients already use.
//!
//! **Read-only by construction.** The server holds no key, has no route that signs, submits,
//! starts or stops anything, and every panel is the JSON its command prints (`misaka.<noun>.v1`),
//! so the page has no second implementation of any answer.
//!
//! It binds loopback. A `Host` header that names anything else is refused, which is what stops a
//! page on another site from reading it through DNS rebinding. `--public` binds every interface
//! and then requires the token it prints on every request.

use crate::operator::profile::Profile;
use crate::{CliError, CliResult, exit};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PAGE: &str = include_str!("dashboard.html");
/// The dashboard's port: the gateway's 8790, plus one.
pub(crate) const DEFAULT_LISTEN: &str = "127.0.0.1:8791";

/// What a request asked for, parsed; only GET exists.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Page,
    Status,
    Doctor,
    Work(String),
    Logs { work: String, events: bool },
    NotFound,
}

/// `GET /api/work/3f9a?token=…` → the route and the query's token.
pub(crate) fn route(target: &str) -> (Route, Option<String>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let param =
        |name: &str| query.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == name).map(|(_, v)| percent_decode(v)));
    let token = param("token");
    let route = match path {
        "/" | "/index.html" => Route::Page,
        "/api/status" => Route::Status,
        "/api/doctor" => Route::Doctor,
        "/api/logs" => Route::Logs { work: param("work").unwrap_or_default(), events: param("events").is_some_and(|v| v == "1") },
        p => match p.strip_prefix("/api/work/") {
            Some(id) if !id.is_empty() => Route::Work(percent_decode(id)),
            _ => Route::NotFound,
        },
    };
    (route, token)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(v) =
                bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// **Only this host's names for this port.** A browser sends the name it resolved: a rebinding
/// attack's page sends its own domain, and is refused here before anything is read.
pub(crate) fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else { return false };
    let host = host.trim().to_ascii_lowercase();
    [format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("[::1]:{port}")].contains(&host)
}

fn response(code: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let mut out = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         Referrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nX-Content-Type-Options: nosniff\r\n\
         Content-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; \
         img-src data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

fn json_response(v: &serde_json::Value) -> Vec<u8> {
    response(200, "application/json", serde_json::to_string(v).unwrap_or_default().as_bytes())
}

/// `misaka dashboard [--listen host:port] [--public]`.
pub(crate) async fn run(listen: &str, public: bool, reprofile: &dyn Fn() -> Result<Profile, CliError>) -> CliResult {
    let addr: std::net::SocketAddr =
        listen.parse().map_err(|e| CliError::new(exit::CONFIG, format!("--listen {listen}: {e} (host:port, e.g. 127.0.0.1:8791)")))?;
    let (addr, token) = if public {
        let mut bytes = [0u8; 16];
        // The clock and the pid are not a secret; the OS's generator is, and without it there is no
        // public dashboard.
        if !getrandom(&mut bytes) {
            return Err(CliError::new(exit::HOST, "--public needs a random token and this host's /dev/urandom cannot be read"));
        }
        let token = faster_hex::hex_string(&bytes);
        (std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), addr.port()), Some(token))
    } else {
        if !addr.ip().is_loopback() {
            return Err(CliError::new(
                exit::CONFIG,
                format!(
                    "{addr} is not a loopback address: the dashboard binds this host only unless --public (which requires a token)"
                ),
            ));
        }
        (addr, None)
    };
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        CliError::new(exit::HOST, format!("bind {addr}: {e} (another dashboard running? pick another port with --listen)"))
    })?;
    match &token {
        Some(t) => {
            println!("MISAKA mining dashboard (PUBLIC — every request needs the token): http://<this host>:{}/?token={t}", addr.port())
        }
        None => println!("MISAKA mining dashboard: http://{addr}/   (read-only; Ctrl-C stops it)"),
    }
    let port = addr.port();
    loop {
        let (mut stream, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut buf = vec![0u8; 8192];
        let n = match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await {
            Ok(Ok(n)) => n,
            _ => continue,
        };
        let head = String::from_utf8_lossy(&buf[..n]).into_owned();
        let mut lines = head.split("\r\n");
        let request = lines.next().unwrap_or_default();
        let mut parts = request.split_whitespace();
        let (method, target) = (parts.next().unwrap_or_default(), parts.next().unwrap_or("/"));
        let host =
            lines.find_map(|l| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case("host")).map(|(_, v)| v.trim().to_string()));
        let (route, query_token) = route(target);
        let reply = if method != "GET" {
            response(400, "text/plain", b"only GET: this page changes nothing")
        } else if token.is_none() && !host_allowed(host.as_deref(), port) {
            response(403, "text/plain", b"this dashboard answers only to 127.0.0.1 and localhost")
        } else if token.is_some() && query_token != token {
            response(403, "text/plain", b"a public dashboard needs its token")
        } else {
            answer(route, reprofile).await
        };
        let _ = stream.write_all(&reply).await;
        let _ = stream.shutdown().await;
    }
}

async fn answer(route: Route, reprofile: &dyn Fn() -> Result<Profile, CliError>) -> Vec<u8> {
    let timeout = Duration::from_secs(5);
    let profile = match route {
        Route::Page => return response(200, "text/html", PAGE.as_bytes()),
        Route::NotFound => return response(404, "text/plain", b"not found"),
        _ => match reprofile() {
            Ok(p) => p,
            Err(e) => return json_response(&serde_json::json!({ "error": e.msg })),
        },
    };
    match route {
        Route::Status => {
            let (snap, view, _) = crate::operator::status::read(profile, timeout).await;
            json_response(&crate::operator::status::document(&snap, &view))
        }
        Route::Doctor => json_response(&crate::operator::doctor::json(profile, &[], false, timeout).await.0),
        Route::Work(id) => match crate::operator::work_cmd::show_json(profile, &id, timeout).await {
            Ok(v) => json_response(&v),
            Err(e) => response(404, "text/plain", e.msg.as_bytes()),
        },
        Route::Logs { work, events } => {
            let needle = work.trim().trim_start_matches("job:").to_ascii_lowercase();
            if !needle.is_empty() && (needle.len() < 8 || !needle.bytes().all(|b| b.is_ascii_hexdigit())) {
                return response(400, "text/plain", b"work: eight hex or more");
            }
            json_response(&serde_json::json!(crate::operator::logs::collect(&profile, &needle, events, 400)))
        }
        Route::Page | Route::NotFound => unreachable!("answered above"),
    }
}

/// Bytes from the OS's generator, for a public dashboard's token; false when there is none.
fn getrandom(out: &mut [u8]) -> bool {
    use std::io::Read;
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(out)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_routed_by_its_path_and_its_query() {
        assert_eq!(route("/").0, Route::Page);
        assert_eq!(route("/api/status?token=ab").1.as_deref(), Some("ab"));
        assert_eq!(route("/api/work/job%3A5b1e09d2").0, Route::Work("job:5b1e09d2".into()));
        assert_eq!(route("/api/logs?work=3f9a1c2e&events=1").0, Route::Logs { work: "3f9a1c2e".into(), events: true });
        assert_eq!(route("/api/work/").0, Route::NotFound);
        assert_eq!(route("/etc/passwd").0, Route::NotFound);
    }

    /// DNS rebinding sends the attacker's host name: only this host's names for this port pass.
    #[test]
    fn only_this_hosts_names_for_this_port_are_answered() {
        assert!(host_allowed(Some("127.0.0.1:8791"), 8791));
        assert!(host_allowed(Some("LOCALHOST:8791"), 8791));
        assert!(!host_allowed(Some("evil.example:8791"), 8791));
        assert!(!host_allowed(Some("127.0.0.1:8790"), 8791), "another port is another service");
        assert!(!host_allowed(None, 8791), "no Host header, no answer");
    }

    #[test]
    fn the_page_is_the_embedded_file_and_never_asks_to_change_anything() {
        assert!(PAGE.contains("<title>MISAKA mining</title>"));
        for write in ["method: \"POST\"", "method:'POST'", "/api/start", "/api/stop"] {
            assert!(!PAGE.contains(write), "the page must not write: {write}");
        }
    }
}
