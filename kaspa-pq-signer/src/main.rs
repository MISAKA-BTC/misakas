//! kaspa-pq remote signer / HSM daemon (ADR-0015, audit H-04).
//!
//! A standalone process that holds the ML-DSA-87 validator key(s) and answers sign requests over a
//! Unix domain socket, enforcing a signing policy + an equivocation guard + a tamper-evident audit
//! log (all in [`kaspa_pq_signer::SignerState`]; the wire loop is [`kaspa_pq_signer::transport`]).
//! The validator node connects locally and never sees the key — node compromise cannot directly
//! exfiltrate it or equivocate. The socket's filesystem permissions (0700, owner-only) are the
//! node-local authentication boundary (ADR-0015).

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    unix_daemon::run()
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("kaspa-pq-signer is a Unix-domain-socket daemon and only runs on Unix targets.");
    std::process::ExitCode::FAILURE
}

#[cfg(unix)]
mod unix_daemon {
    use std::{
        os::unix::{fs::PermissionsExt, net::UnixListener},
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use clap::Parser;
    use kaspa_consensus_core::dns_finality::{HostId, SignerPolicy};
    use kaspa_pq_signer::{SignerState, transport::serve_connection};
    use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};

    #[derive(Parser, Debug)]
    #[command(name = "kaspa-pq-signer", about = "kaspa-pq remote signer / HSM daemon (ADR-0015)")]
    struct Args {
        /// Unix domain socket path to listen on (created; replaced if a stale file exists).
        #[arg(long, default_value = "/tmp/kaspa-pq-signer.sock")]
        socket: String,
        /// Validator key seed file(s) (hex of 32 bytes), repeatable for a multi-tenant signer.
        #[arg(long = "key", required = true)]
        keys: Vec<String>,
        /// Directory for the equivocation logs + the audit log (created if absent).
        #[arg(long, default_value = "./kpq-signer-state")]
        state_dir: String,
        /// Signing policy: `permissive` (sign all well-formed), `audit-only` (log conflicts, still
        /// sign), or `strict` (refuse equivocating attestations). Default: strict.
        #[arg(long, default_value = "strict")]
        policy: String,
    }

    fn parse_policy(s: &str) -> Result<SignerPolicy, String> {
        match s.to_ascii_lowercase().as_str() {
            "permissive" => Ok(SignerPolicy::Permissive),
            "audit-only" | "auditonly" | "audit" => Ok(SignerPolicy::AuditOnly),
            "strict" => Ok(SignerPolicy::Strict),
            other => Err(format!("unknown --policy '{other}' (want permissive|audit-only|strict)")),
        }
    }

    pub fn run() -> std::process::ExitCode {
        kaspa_core::log::init_logger(None, "info");
        let args = Args::parse();

        let policy = match parse_policy(&args.policy) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return std::process::ExitCode::FAILURE;
            }
        };

        // Load the validator key(s).
        let mut keys: Vec<ValidatorKey> = Vec::new();
        for path in &args.keys {
            match load_validator_seed(path) {
                Ok(seed) => {
                    let k = ValidatorKey::from_seed(seed);
                    log::info!("[signer] loaded key for validator {}", k.validator_id);
                    keys.push(k);
                }
                Err(e) => {
                    eprintln!("cannot load key '{path}': {e}");
                    return std::process::ExitCode::FAILURE;
                }
            }
        }

        // The signer's own host identity = keyed BLAKE2b over all served validator ids → a 32-byte
        // HostId. Stable + non-secret; only used to attribute the ack/audit records to this signer.
        let server_identity: HostId = {
            let mut st = blake2b_simd::Params::new().hash_length(32).key(b"kaspa-pq-signer-id").to_state();
            for k in &keys {
                st.update(k.validator_id.as_byte_slice());
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(st.finalize().as_bytes());
            HostId::from_bytes(out)
        };

        let state = match SignerState::new(keys, policy, PathBuf::from(&args.state_dir), server_identity) {
            Ok(s) => Arc::new(Mutex::new(s)),
            Err(e) => {
                eprintln!("cannot initialize signer state: {e}");
                return std::process::ExitCode::FAILURE;
            }
        };

        // Bind the socket (replace a stale file) and lock it down to owner-only (the node-local
        // authentication boundary).
        let _ = std::fs::remove_file(&args.socket);
        let listener = match UnixListener::bind(&args.socket) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("cannot bind socket {}: {e}", args.socket);
                return std::process::ExitCode::FAILURE;
            }
        };
        if let Err(e) = std::fs::set_permissions(&args.socket, std::fs::Permissions::from_mode(0o700)) {
            log::warn!("[signer] could not restrict socket perms: {e}");
        }
        log::info!("[signer] listening on {} (policy {:?})", args.socket, policy);

        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || serve_connection(stream, &state, server_identity));
                }
                Err(e) => log::warn!("[signer] accept failed: {e}"),
            }
        }
        std::process::ExitCode::SUCCESS
    }
}
