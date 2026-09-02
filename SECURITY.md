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

### 3. PALW free-prompt gateway and worker — the host half (ADR-0079)

A node that answers free prompts runs a **public entrance that parses attacker-chosen text and
hands it to a model**, on a host that may also hold a bond. The posture is enforced locally,
reported honestly by `misaka node security-report`, and **committed nowhere**: the chain cannot
observe whether a host ran confined, and a court that cannot compute a verdict is a vote.

**The model process starts with nothing.** `misaka-palw-agent` and `misaka-palw-gateway` spawn every
worker — the job, the boot manifest probe and the boot selftest alike — with `env_clear()` and the
in-tree constant `PALW_WORKER_ENV_ALLOWLIST` (the `MISAKA_PALW_*` artifact variables the worker
actually reads, plus pinned `LC_ALL`/`LANG`/`LC_NUMERIC`/`TZ`). **`PATH` is deliberately absent**:
the supervisor spawns by absolute path, so an inherited `PATH` would only be an execution vector.
The working directory is an explicit `0700` scratch dir, never the operator's home or the datadir.
Adding a name to the allowlist is a source change and a review, not a config edit.

**Every job has a resident ceiling and a deadline, and exceeding either is a failed job — never a
dead node.** `PALW_WORKER_MAX_RESIDENT_BYTES` (override:
`MISAKA_PALW_WORKER_MAX_RESIDENT_BYTES`, or `--worker-max-resident-bytes`) is enforced by a
delegated cgroup v2 `memory.max` when `MISAKA_PALW_WORKER_CGROUP` names one, and by a supervisor
resident watchdog otherwise. It is **not** `RLIMIT_AS`: the hybrid class maps a 33 GiB artifact and
an address-space cap would kill the worker while it was still mapping.

**Accepted conditions for the gateway** (the same shape as §1, extending it rather than replacing
it):

- **Default bind is loopback** (`127.0.0.1:8790`).
- **A non-loopback `--listen` fails at startup** unless `MISAKA_PALW_ALLOW_PUBLIC_GATEWAY=1`. The
  intended production pattern is an **authenticating reverse proxy** in front of a loopback-bound
  gateway.
- **A public bind on a host whose confinement backend is `none` fails unconditionally**, and the
  acknowledgement variable does not override it. That is the one place where a stranger chooses the
  model's input. (Today no platform backend ships, so this rule is the load-bearing one.)
- **No wildcard CORS**, and no secret-shaped field in any response DTO.
- **The gateway refuses to boot if a signing secret is reachable in its own view** — a
  `MISAKA_*_SEED`-style variable, or a 32-byte seed-shaped file beside the identity file or in the
  outbox. It holds the executor **public** key only; the ML-DSA-87 signature belongs to the signer
  sidecar.
- **Mandatory bounds, not defaults**: request body ≤ 1 MiB, rendered prompt ≤ 64 KiB, decode cap
  ≤ 4,096, at most 64 connections, **one job slot**, and a **bounded in-flight queue** of 8 —
  past which the answer is a 503, never a wait. A flag may lower any of these and never raise one.
- **A public job spends the OPERATOR's exposure, and the spend is bounded.** A stranger's prompt
  becomes the operator's claim, so `--bond-exposure-room-sompi`, `--claim-exposure-sompi` and
  `--public-job-budget-permille` bound what strangers may spend per 24 h window; past the budget —
  or under `--answer-never-commit` — the gateway **answers and does not commit**. A queued
  commitment expires with its anchor and is retired rather than submitted stale. `/health` names
  the loss bound: at most `claim_exposure` per claim, and at most the `FreePromptExposureCeiling`
  ratio (500‰ on the RC) of collateral in flight.
- **A per-source rate limit is secondary, by design.** Sources share addresses behind proxies; the
  binding limits are the single job slot, the bounded queue and the budget.
- **Nothing logs a prompt.** Gateway, supervisor, worker and seat log token counts and roots, never
  prompt text or prompt ids.

**What this does not buy:** nothing here makes a dishonest executor honest, and nothing here is
visible to a peer. A node that lies about its posture is exactly as convictable as before — through
its roots — and exactly as unconvictable for its posture.

### 4. Other operator notes

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
