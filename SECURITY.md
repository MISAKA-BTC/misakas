# Security

## Reporting a vulnerability

Please report suspected vulnerabilities privately to the maintainers rather than opening a public
issue. Include a description, affected component, and reproduction steps where possible.

---

## Operator security model

This document records the security posture of the operator-facing components (mining bridge, remote
signer, miners) and, in particular, two decisions that are **accepted by design** with explicit
conditions. They are deliberate trade-offs, not unaddressed findings.

### 1. Mining bridge — dashboard / metrics exposure (accepted-by-design)

The bridge's web dashboard and Prometheus endpoints expose operational config and topology
(`/api/config`, `/api/status`, `/api/stats`, `/metrics`) and are **not authenticated**. Rather than
build an auth system into an operator tool, exposure is controlled by binding and explicit opt-in:

- **Default bind is loopback.** A bare port (e.g. `":3030"`) binds `127.0.0.1`, not all interfaces.
  The Stratum mining ports still bind `0.0.0.0` (miners must reach them).
- **Public bind requires explicit acknowledgement.** Binding a dashboard/metrics endpoint to a
  non-loopback address **fails at startup** unless `RKSTRATUM_ALLOW_PUBLIC_DASHBOARD=1` is set. The
  intended production pattern for remote access is an **authenticating reverse proxy** in front of a
  loopback-bound dashboard.
- **No wildcard CORS.** `/api/*` responses do not send `Access-Control-Allow-Origin: *`, so a page on
  another origin cannot read them from the operator's browser.
- **Config read is a public-safe DTO.** `/api/config` returns only operational config (node address,
  ports, share/diff settings) — never secrets, tokens, or mnemonics. If a secret-like field is ever
  added to the config it MUST be excluded from this response.
- **Config write is off by default and CSRF-guarded.** `POST /api/config` returns 403 unless
  `RKSTRATUM_ALLOW_CONFIG_WRITE=1`, rejects cross-origin requests (allowing loopback and same-origin
  from the server's own concrete bind host), and bounds the request body.

**Accepted conditions** (record these as the design contract):

- Dashboard / Prometheus default to **loopback only**.
- A **public bind requires `RKSTRATUM_ALLOW_PUBLIC_DASHBOARD=1`** (otherwise the server refuses to
  start) — and should still sit behind an authenticating reverse proxy.
- `/api/config` responses are limited to a **public-safe DTO** (no secrets/tokens/private paths).
- **No wildcard CORS.**
- **Config write always requires CSRF + (for public access) reverse-proxy auth.** The CSRF guard
  accepts loopback and same-origin from a concrete bind host; a wildcard (`0.0.0.0`) bind has no
  single canonical host, so **public config-write must be performed through an authenticating reverse
  proxy that presents a same-origin request** — direct cross-origin/public writes are rejected by
  design. Do **not** expose the dashboard directly to the public internet.

### 2. Remote signer (`kaspa-pq-signer`) — node-local trust model (accepted-by-design)

The signer holds the ML-DSA-87 validator key and answers sign requests over a Unix-domain socket. By
design (ADR-0015) the signer signs the digest + context it is handed; it does **not** enforce a
purpose→context allowlist by default, because that would couple it to the validator's signing paths.
Its authentication boundary is **node-local**:

- Socket lives in a `0700` directory (`$XDG_RUNTIME_DIR` by default), created with a tightened umask
  before bind (no bind-then-chmod race); a permission failure is fail-closed.
- State dir is `0700`, the audit log `0600`.
- Every connection's peer credentials are checked (Linux/Android via `SO_PEERCRED`, the BSDs/macOS via
  `getpeereid(2)`): only the signer's own UID (or an explicit `--allowed-uid`) may connect by default.
  A handshake read timeout reaps connect-and-hold attempts.
- An over-long (>255-byte) signing context is refused in-band (never panics), and the request lock is
  poison-tolerant, so one bad request cannot wedge the daemon.

**Optional policy hooks (off by default):**

- `--allowed-uid <uid>` (repeatable) — restrict connecting client UIDs to an explicit allowlist.
- `--deny-purpose <transaction|attestation|unbond|takeover>` (repeatable) — refuse signing for a
  purpose. A validator-only signer can pass `--deny-purpose transaction` so it never signs arbitrary
  transactions.

**Accepted condition:** the signer trusts processes running as its own UID (or the configured UID
allowlist) on the same host. Run it as a dedicated service account, not a shared login. A future
strict purpose→context policy can be layered on via the hooks above without changing the default.

### 3. Other operator notes

- **Stratum listener** enforces global and per-IP connection caps (`max_connections`,
  `max_connections_per_ip`), a pre-auth idle disconnect, a hard pre-auth authorize deadline (closes
  slow-trickle slot-holds), and a per-message length cap.
- **Prometheus metrics cardinality:** the mined-block gauge is low-cardinality, and the `worker`/`miner`
  labels are sanitized and `ip` carries no port. The `wallet` label is still per-(valid)-address, so on
  a **public** Stratum a client could grow series by authorizing many distinct addresses. This is an
  operational (not consensus/fund) concern — run a public pool's metrics endpoint behind monitoring
  that bounds/aggregates series, or drop the `wallet`/`ip` labels if you do not need per-wallet metrics.
- **Validator keys** are written with `O_CREAT|O_EXCL` at mode `0600` (no clobber, no symlink follow);
  loading a group/world-readable seed file logs a warning.
- **Miners** refuse to start when no payout address is configured (they will not silently mine to an
  unspendable placeholder); pass `--allow-burn` only for PoW smoke tests.
- **Supply chain:** GitHub Actions are pinned to commit SHAs and Docker base images to manifest
  digests; Dependabot maintains both. CI runs a hard-failing dependency advisory gate (`cargo-deny`).
