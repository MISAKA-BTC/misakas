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
    use kaspa_consensus_core::dns_finality::{HostId, SignerPolicy, SigningPurpose};
    use kaspa_pq_signer::{SignerState, transport::serve_connection};
    use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};

    #[derive(Parser, Debug)]
    #[command(name = "kaspa-pq-signer", about = "kaspa-pq remote signer / HSM daemon (ADR-0015)")]
    struct Args {
        /// Unix domain socket path to listen on (created; replaced if a stale file exists). Default:
        /// `$XDG_RUNTIME_DIR/kaspa-pq-signer.sock` (a per-user 0700 dir), else a 0700 per-uid dir under
        /// the temp dir — never a world-writable path such as bare `/tmp`.
        #[arg(long)]
        socket: Option<String>,
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
        /// Restrict which client UIDs may connect (repeatable). Default: only the signer's own UID
        /// (peer-credential check via getpeereid). Set this to share the signer with another service
        /// account, or to lock it to a specific UID.
        #[arg(long = "allowed-uid")]
        allowed_uids: Vec<u32>,
        /// Refuse to sign for these purposes (repeatable): `transaction`, `attestation`, `unbond`,
        /// `takeover`, `palw-attempt`, `palw-fp-commitment`, `palw-fp-spend`, `palw-derived`.
        /// E.g. a validator-only signer can pass `--deny-purpose transaction` so it never signs
        /// arbitrary transactions, and a signer that must never spend a free-prompt quantum can
        /// pass `--deny-purpose palw-fp-spend`. Default: none denied.
        #[arg(long = "deny-purpose")]
        deny_purposes: Vec<String>,
    }

    /// **The name `--deny-purpose` accepts for a purpose, matched exhaustively** (mainnet audit,
    /// 2026-09-05).
    ///
    /// `SigningPurpose` has grown to eight variants and the parser named four of them, so the four
    /// PALW purposes — the block-production attempt, the free-prompt commitment, the free-prompt
    /// QUANTUM SPEND and the derived artifact — could not be denied on any signer or HSM. An
    /// operator who wanted a signer that attests and never spends had no way to say it. The match
    /// here is exhaustive, so a purpose added tomorrow does not compile until it is nameable.
    fn purpose_cli_name(purpose: SigningPurpose) -> &'static str {
        match purpose {
            SigningPurpose::Transaction => "transaction",
            SigningPurpose::Attestation => "attestation",
            SigningPurpose::Unbond => "unbond",
            SigningPurpose::TakeoverToken => "takeover",
            SigningPurpose::PalwAttemptV2 => "palw-attempt",
            SigningPurpose::PalwFpCommitmentV3 => "palw-fp-commitment",
            SigningPurpose::PalwFpSpendV3 => "palw-fp-spend",
            SigningPurpose::PalwDerivedArtifactV1 => "palw-derived",
        }
    }

    /// Every purpose a signer can be asked for. The array length is the only hand-kept fact, and
    /// `every_signing_purpose_can_be_denied` checks it against the discriminants.
    const ALL_SIGNING_PURPOSES: [SigningPurpose; 8] = [
        SigningPurpose::Transaction,
        SigningPurpose::Attestation,
        SigningPurpose::Unbond,
        SigningPurpose::TakeoverToken,
        SigningPurpose::PalwAttemptV2,
        SigningPurpose::PalwFpCommitmentV3,
        SigningPurpose::PalwFpSpendV3,
        SigningPurpose::PalwDerivedArtifactV1,
    ];

    fn parse_purpose(s: &str) -> Result<SigningPurpose, String> {
        let want = s.to_ascii_lowercase();
        // The short aliases that shipped, kept so existing unit files keep working.
        match want.as_str() {
            "tx" => return Ok(SigningPurpose::Transaction),
            "attest" => return Ok(SigningPurpose::Attestation),
            "takeover-token" => return Ok(SigningPurpose::TakeoverToken),
            _ => {}
        }
        ALL_SIGNING_PURPOSES.into_iter().find(|p| purpose_cli_name(*p) == want).ok_or_else(|| {
            let names: Vec<&str> = ALL_SIGNING_PURPOSES.into_iter().map(purpose_cli_name).collect();
            format!("unknown --deny-purpose '{s}' (want {})", names.join("|"))
        })
    }

    fn parse_policy(s: &str) -> Result<SignerPolicy, String> {
        match s.to_ascii_lowercase().as_str() {
            "permissive" => Ok(SignerPolicy::Permissive),
            "audit-only" | "auditonly" | "audit" => Ok(SignerPolicy::AuditOnly),
            "strict" => Ok(SignerPolicy::Strict),
            other => Err(format!("unknown --policy '{other}' (want permissive|audit-only|strict)")),
        }
    }

    /// Resolve the default socket path to a path inside a 0700, owner-only directory: prefer
    /// `$XDG_RUNTIME_DIR` (already per-user 0700), else create a per-uid dir under the system temp dir.
    /// Avoids placing the socket directly in a world-writable directory such as bare `/tmp`.
    fn default_socket_path() -> Result<PathBuf, String> {
        if let Some(dir) = std::env::var("XDG_RUNTIME_DIR").ok().filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(dir).join("kaspa-pq-signer.sock"));
        }
        let uid = unsafe { libc::geteuid() };
        let dir = std::env::temp_dir().join(format!("kaspa-pq-signer-{uid}"));
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create signer socket dir {}: {e}", dir.display()))?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("cannot chmod 700 {}: {e}", dir.display()))?;
        Ok(dir.join("kaspa-pq-signer.sock"))
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

        // Optional purpose denylist (e.g. validator-only signer denying `transaction`).
        let denied_purposes = {
            let mut v = Vec::new();
            for p in &args.deny_purposes {
                match parse_purpose(p) {
                    Ok(purpose) => v.push(purpose),
                    Err(e) => {
                        eprintln!("{e}");
                        return std::process::ExitCode::FAILURE;
                    }
                }
            }
            v
        };

        let state = match SignerState::new(keys, policy, PathBuf::from(&args.state_dir), server_identity) {
            Ok(mut s) => {
                if !denied_purposes.is_empty() {
                    log::info!("[signer] denying purposes: {denied_purposes:?}");
                    s.set_denied_purposes(denied_purposes);
                }
                Arc::new(Mutex::new(s))
            }
            Err(e) => {
                eprintln!("cannot initialize signer state: {e}");
                return std::process::ExitCode::FAILURE;
            }
        };

        // Resolve the socket path (XDG/0700 dir by default; see default_socket_path).
        let socket_path = match &args.socket {
            Some(s) => PathBuf::from(s),
            None => match default_socket_path() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{e}");
                    return std::process::ExitCode::FAILURE;
                }
            },
        };

        // Tighten the umask BEFORE bind so the socket is created owner-only from the start — this
        // closes the bind-then-chmod race window where the socket briefly existed at the process
        // umask (potentially world-connectable). The explicit chmod below is then belt-and-suspenders.
        unsafe { libc::umask(0o077) };

        // Bind the socket (replace a stale file) and lock it down to owner-only (the node-local
        // authentication boundary).
        let _ = std::fs::remove_file(&socket_path);
        let listener = match UnixListener::bind(&socket_path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("cannot bind socket {}: {e}", socket_path.display());
                return std::process::ExitCode::FAILURE;
            }
        };
        // Fail-closed: if we cannot prove the socket is owner-only, do not serve (the file mode is the
        // authentication boundary; a readable/connectable socket would be an open signing oracle).
        if let Err(e) = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o700)) {
            eprintln!("cannot restrict socket perms on {}: {e}", socket_path.display());
            let _ = std::fs::remove_file(&socket_path);
            return std::process::ExitCode::FAILURE;
        }
        log::info!("[signer] listening on {} (policy {:?})", socket_path.display(), policy);

        let allowed_uids = args.allowed_uids.clone();
        if !allowed_uids.is_empty() {
            log::info!("[signer] restricting client UIDs to {allowed_uids:?}");
        }
        // Audit M-03: bound concurrent connections. Each connection holds a dedicated
        // OS thread (and serve_connection may idle a long-lived client post-handshake),
        // so without a cap a same-uid client could open many sockets and exhaust
        // threads. The cap bounds both the thread count and the number of idle holders
        // (the peer is already same-uid/allowlisted; this is local-DoS hardening).
        use std::sync::atomic::{AtomicUsize, Ordering};
        const MAX_CONCURRENT_CONNECTIONS: usize = 64;
        let active_conns = Arc::new(AtomicUsize::new(0));
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    if active_conns.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT_CONNECTIONS {
                        active_conns.fetch_sub(1, Ordering::AcqRel);
                        log::warn!("[signer] at the {MAX_CONCURRENT_CONNECTIONS}-connection cap; dropping a new connection");
                        drop(stream);
                        continue;
                    }
                    let state = Arc::clone(&state);
                    let allowed_uids = allowed_uids.clone();
                    let active = Arc::clone(&active_conns);
                    std::thread::spawn(move || {
                        serve_connection(stream, &state, server_identity, &allowed_uids);
                        active.fetch_sub(1, Ordering::AcqRel);
                    });
                }
                Err(e) => log::warn!("[signer] accept failed: {e}"),
            }
        }
        std::process::ExitCode::SUCCESS
    }

    #[cfg(test)]
    mod deny_purpose_tests {
        use super::*;

        /// **Every purpose a signer can be asked for can be denied.** The four PALW purposes were
        /// unnameable, so the free-prompt quantum spend could not be refused on an HSM.
        #[test]
        fn every_signing_purpose_can_be_denied() {
            for purpose in ALL_SIGNING_PURPOSES {
                let name = purpose_cli_name(purpose);
                assert_eq!(parse_purpose(name), Ok(purpose), "--deny-purpose {name} must name {purpose:?}");
                assert_eq!(parse_purpose(&name.to_ascii_uppercase()), Ok(purpose), "and case must not matter");
            }
            // The array is the whole enum: discriminants 0..8, each mapped to a distinct name.
            let names: std::collections::BTreeSet<&str> = ALL_SIGNING_PURPOSES.into_iter().map(purpose_cli_name).collect();
            assert_eq!(names.len(), ALL_SIGNING_PURPOSES.len(), "no two purposes share a name");
            let mut discriminants: Vec<u8> = ALL_SIGNING_PURPOSES.iter().map(|p| *p as u8).collect();
            discriminants.sort_unstable();
            assert_eq!(
                discriminants,
                (0u8..8).collect::<Vec<_>>(),
                "the array is the whole enum: every discriminant present, none twice"
            );
            // The four the audit found, named individually.
            for name in ["palw-attempt", "palw-fp-commitment", "palw-fp-spend", "palw-derived"] {
                assert!(parse_purpose(name).is_ok(), "{name} must be deniable");
            }
            // The shipped aliases still work, and an unknown name still fails with the full list.
            assert_eq!(parse_purpose("tx"), Ok(SigningPurpose::Transaction));
            let e = parse_purpose("nonsense").expect_err("unknown purposes are refused");
            assert!(e.contains("palw-fp-spend"), "the error lists what is available: {e}");
        }
    }
}
