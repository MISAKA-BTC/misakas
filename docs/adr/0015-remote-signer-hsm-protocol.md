# ADR-0015: Remote-Signer / HSM Protocol for Validator Signing

> **✅ SOFTWARE SIGNER DAEMON IMPLEMENTED (audit H-04).** The remote-signer protocol now ships as the **`kaspa-pq-signer`** crate: a standalone Unix-domain-socket daemon that holds the ML-DSA-87 validator key(s) *outside* the validator process and answers length-prefixed Borsh sign requests, enforcing (a) purpose/digest typing — the `SignerMessageDigest` enum (`Transaction(Hash64)` / `Attestation(Hash)` / `Unbond` / `TakeoverToken`) + `purpose_matches_digest` (audit H-03); (b) a per-validator signing **policy** (`Permissive` / `AuditOnly` / `Strict`); (c) a **`Strict`-policy anti-equivocation guard** backed by the crash-consistent, fsync'd `SignedEpochStore` (the authoritative equivocation record moves from the validator client to the signer); and (d) a **tamper-evident, hash-chained audit log** that replays + re-verifies its head on restart. The wire transport and a blocking **`SignerClient`** (the validator's production signing path) are exercised end-to-end over a real socket; the listening socket is locked to `0700` (owner-only) as the node-local authentication boundary, and the daemon is **fail-closed** (any policy/key/format problem returns a structured `SignerError`, never an unintended signature). **Still out of scope:** a *hardware*-HSM / PKCS#11 backend (the daemon is a **software** signer; `SignerError::HsmError(code, msg)` is reserved for a future PKCS#11 bridge) and an automated failover / HA workflow. The local key-file signer (ADR-0010) remains the default; operators opt into the daemon.
>
> **⚠️ ML-DSA-65-era design doc (HISTORICAL — audit M-03).** The signature scheme is now **ML-DSA-87** (pk 2592 B / sig 4627 B) per [ADR-0019](0019-mldsa87-migration.md); the `ML-DSA-65` / `1952` / `3309` values below are the original draft and are **not current consensus**.

Status: Accepted — software signer daemon implemented (`kaspa-pq-signer`); hardware-HSM / PKCS#11 backend + HA failover deferred
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0002](0002-mldsa65-p2pkh.md) (ML-DSA-65 signing
            scheme used over the remote-signer protocol),
            [ADR-0008](0008-hash64-consensus-identity.md) (Hash64
            for validator and audit identifiers),
            [ADR-0009](0009-dns-probabilistic-finality.md)
            (SlashingEvidencePayload — the equivocation guard
            this ADR optionally relocates from the validator to
            the signer),
            [ADR-0010](0010-validator-node-architecture.md)
            §"Negative" (the "key management on a hot node"
            future-ADR pointer this ADR resolves),
            [ADR-0011](0011-validator-deployment-and-equivocation-safety.md)
            (the `SignedEpochRecord` and `check_signed_epoch_record`
            guard whose authoritative location can move to the
            signer in Strict policy mode),
            [ADR-0014](0014-coordinated-failover-protocol.md)
            (`TakeoverToken` is one of the message types the
            signer can be asked to sign — the protocol surface
            covers all ML-DSA-65 use sites uniformly).

## Context

[ADR-0010 §"Negative"](0010-validator-node-architecture.md) lists
as a downside of the validator architecture:

> Key management on a hot node. The validator service reads the
> ML-DSA-65 signing key from disk at startup. Operators who want
> HSM-backed signing need a follow-up ADR introducing an
> RPC-based remote-signer mode; that mode is out of scope here.

This ADR is that follow-up.

Operators with stronger key-custody requirements (regulated
exchanges, custodians, large stakers) want the validator
**signing key** to live outside the validator process — ideally
inside a hardware security module (HSM) or on a dedicated,
locked-down host. The validator process should originate sign
requests but never see the private key material.

The DNS overlay design (ADR-0009) makes this particularly
valuable: the validator key is a slashing risk (ADR-0009
§"`SlashingEvidencePayload`") and a hot-network presence is
required for it to fulfil its purpose. Moving custody to a
purpose-built signer reduces the attack surface from "the entire
node host" to "the signer process plus its key store" — a
materially smaller and easier-to-audit perimeter.

This ADR specifies the **wire protocol** between a validator
client and a remote signer, the **policy model** the signer
enforces, and the **HSM integration surface** the signer
exposes to plug in hardware backends. The protocol covers
**every** ML-DSA-65 use site in kaspa-pq — transaction signing,
attestation signing, and takeover-token signing — uniformly, so
a single signer process can serve every signing need an operator
has.

## Decision

### Topology and transport

```text
host A (signer-equipped validator host)
┌──────────────────────────────────────────┐
│ kaspa-pq-node                            │
│ kaspa-pq-validator                       │
│   │                                      │
│   │  (Borsh-framed requests over a       │
│   │   Unix domain socket; signer file    │
│   │   is the only authentication needed) │
│   ▼                                      │
│ kaspa-pq-signer (separate process)       │
│   ├── policy enforcement                 │
│   ├── audit log                          │
│   └── signing backend:                   │
│       ├── SoftwareKey (default;          │
│       │   in-memory key, AES-256-GCM     │
│       │   encrypted at rest)             │
│       └── HsmAdapter (optional;          │
│           PKCS#11 / vendor SDK)          │
└──────────────────────────────────────────┘
```

Transport choice for v1:

- **Unix domain socket** (default). Same-host only. Authentication
  is by file-mode permissions on the socket path; only the
  `kaspa` user can connect. No encryption is needed — the
  loopback traffic never leaves the host.
- TLS-over-loopback (optional, future): for compliance regimes
  that require encryption-at-rest plus encryption-in-transit
  even on localhost. Out of scope for v1 — same-host plain
  Unix socket is enough for the threat model this ADR
  addresses.
- Network transport (different host): **explicitly out of scope
  for v1.** Network-distributed remote signing requires
  authenticated TLS or mTLS and a clear policy for handling
  network partitions between validator and signer; a follow-up
  ADR (candidate ADR-0017) would cover it.

### Protocol versioning + handshake

```text
SIGNER_PROTOCOL_VERSION = 1
```

Handshake (Borsh-framed; length-prefixed):

```text
client → server: SignerHello {
    protocol_version: u16,        // SIGNER_PROTOCOL_VERSION
    capabilities: u32,            // bitflags
    client_identity: Hash,        // HostId per ADR-0014 — lets the
                                  // signer's audit log attribute
                                  // requests to a specific client
}

server → client: SignerHelloAck {
    protocol_version: u16,        // must match
    capabilities: u32,            // server's capabilities
    server_identity: Hash,        // for round-trip auditability
}
```

Version mismatch closes the connection with a single
`SignerError::ProtocolVersionMismatch` frame and no further
traffic.

Capabilities bitflags (initial set; can grow without a version
bump, hence the bitfield rather than enum):

```text
0x01  CAP_SIGN_TRANSACTION
0x02  CAP_SIGN_ATTESTATION
0x04  CAP_SIGN_TAKEOVER_TOKEN
0x08  CAP_POLICY_STRICT       (server can enforce equivocation
                               guard; see §"Policy model" below)
0x10  CAP_AUDIT_LOG           (server writes append-only audit log)
0x20  CAP_HSM_BACKED          (key material lives in HSM, not RAM)
```

A validator client refuses to start signing attestations unless
the server advertises `CAP_SIGN_ATTESTATION`. If the validator
is configured with `--signer-policy strict`, the client also
requires `CAP_POLICY_STRICT`.

### Request / response cycle

```text
client → server: SignerRequest {
    request_id: u64,              // monotonic per-client; signer
                                  // dedupes by request_id for the
                                  // lifetime of the connection
    validator_id: Hash64,         // which key (the signer may
                                  // hold more than one)
    purpose: SigningPurpose,      // enum — see below
    context: Vec<u8>,             // libcrux sign_ctx ctx parameter
    message_digest: Hash,         // 32-byte BLAKE2b-256 the
                                  // ML-DSA-65 will sign over
    metadata: SignerMetadata,     // structured per-purpose data —
                                  // see below
}

server → client: SignerResponse {
    request_id: u64,              // echoes the request
    result: Result<Vec<u8>,       // 3309 B ML-DSA-65 signature
                   SignerError>,  // or structured failure
}
```

`SigningPurpose` enum (Borsh-stable discriminants — protocol-
level wire format, so renumbering is a hard fork of the
protocol):

```rust
pub enum SigningPurpose {
    Transaction      = 0,
    Attestation      = 1,
    TakeoverToken    = 2,
}
```

`SignerMetadata` is purpose-tagged so the signer can enforce
policy meaningfully:

```rust
pub enum SignerMetadata {
    None,                                       // SigningPurpose::Transaction
    Attestation {                               // SigningPurpose::Attestation
        epoch: u64,
        target_hash: Hash64,
        target_daa_score: u64,
    },
    TakeoverToken {                             // SigningPurpose::TakeoverToken
        yielding_host_id: Hash,
        taking_over_host_id: Hash,
        valid_from_epoch: u64,
        grace_epochs: u8,
    },
}
```

The metadata is **not** part of the message the ML-DSA-65 signs
over — it is in-band hints for the signer's policy engine.
Operators who use `SignerPolicy::Permissive` can omit the
metadata entirely (`SignerMetadata::None` is valid for any
purpose).

`SignerError` enum:

```rust
pub enum SignerError {
    ProtocolVersionMismatch     = 0,
    KeyNotFound                 = 1,
    UnknownPurpose              = 2,
    PolicyViolation { reason }  = 3,
    HsmError { code, message }  = 4,
    RateLimit                   = 5,
    InternalError { message }   = 6,
}
```

### Policy model

The signer enforces one of three policies per validator_id:

| Policy | Behaviour | When to use |
|---|---|---|
| `Permissive` | Sign every well-formed request. No equivocation guard. | Closest to the local-key-file behaviour of ADR-0010; appropriate when the validator client is the only authority and its `SignedEpochRecord` DB is trusted. |
| `AuditOnly` | Sign every request but log warnings to the audit log on policy violations (e.g. would-be equivocation). | Migration path — operators converting from Permissive to Strict can use this mode to discover bugs without breaking production signing. |
| `Strict` | Enforce the ADR-0011 equivocation guard at the signer. Refuse to sign any `Attestation` request whose `metadata.epoch + validator_id` already has a recorded `target_hash | target_daa_score` that differs from the request. | Production for operators who want the strongest possible double-signing prevention. **Moves the authoritative `SignedEpochRecord` store from the validator client to the signer.** |

In `Strict` mode the signer maintains its own per-validator-id
`SignedEpochRecord` store and runs `check_signed_epoch_record`
(ADR-0011) against each `Attestation` request. The
`SignedEpochCheckOutcome` is mapped to:

```text
Allow             → sign, store new record
AllowRebroadcast  → sign (record unchanged)
Block             → refuse with SignerError::PolicyViolation
```

The validator client's local `SignedEpochRecord` DB still works
in Strict mode but is **advisory** — the binding check is at
the signer. This is the right architecture because:
- The signer is the single point of authority (only one process
  can produce a signature with the validator key);
- Multiple validator clients can point at one signer (e.g.
  primary + hot-standby validator clients per the ADR-0014
  failover protocol, both submitting sign requests to the same
  signer process) and the signer's Strict policy makes
  cross-client double-signing impossible.

The TakeoverToken purpose has its own policy check:
- Strict: only sign one TakeoverToken per
  `(validator_id, valid_from_epoch)`; refuse repeats with
  differing `(yielding, taking_over)` pairs.
- The signer never enforces "is this token a sensible
  handoff?" — that is the validator/operator's decision.

### Audit log

When `CAP_AUDIT_LOG` is advertised, the signer writes every
request to an append-only log:

```rust
pub struct SignerAuditRecord {
    pub timestamp_unix_secs: u64,
    pub client_identity: Hash,
    pub request_id: u64,
    pub validator_id: Hash64,
    pub purpose: SigningPurpose,
    pub metadata: SignerMetadata,
    pub message_digest: Hash,         // 32 B
    /// BLAKE2b-512 of the signature bytes — pinned so the
    /// audit log records what was signed without storing the
    /// full 3309 B signature payload.
    pub signature_fingerprint: Hash64,
    pub outcome: SignerOutcome,       // Signed | Refused(SignerError)
}
```

Audit log integrity: each record is BLAKE2b-512-chained to its
predecessor (the previous record's hash is fed into the next
record's hash input), so any post-hoc tampering shifts the
chain and is detectable by a verifier walking the log from a
known-good starting point.

```text
AUDIT_LOG_CHAIN_KEY = b"kaspa-pq-signer-audit-v1"
```

Audit log compaction: out of scope for this ADR. Operators are
expected to rotate the log offline (e.g. weekly into separate
files, each chained from the previous file's terminal hash).

### HSM integration surface

The signer's signing backend is an internal trait that the
default in-memory implementation and any HSM adapter both
implement:

```text
trait SignerBackend {
    fn validator_pubkey(&self, validator_id: Hash64)
        -> Result<MlDsa65PublicKey, SignerError>;

    fn sign_ctx(&self,
        validator_id: Hash64,
        message_digest: &Hash,
        context: &[u8])
        -> Result<Vec<u8>, SignerError>;
}
```

The HSM adapter is **separate from this ADR's wire protocol** —
the protocol does not know or care whether the signer's backend
is software or hardware. Vendor-specific HSM glue (PKCS#11,
proprietary SDKs) lives behind the `SignerBackend` trait.

Reference adapters this ADR pre-commits to:
- `SoftwareKey` — key file encrypted at rest with the operator
  passphrase (Argon2id + ChaCha20-Poly1305, matching Phase 5'
  wallet seed encryption ADR-precedent); decrypted in-memory at
  signer startup. Default.
- `Pkcs11Adapter` — opt-in via build feature; loads a
  PKCS#11 library specified by env var or config. Per-vendor
  quirks documented per-deployment.

A `kaspa-pq-signer` binary built **without** the `pkcs11` feature
runs only `SoftwareKey` — useful for the common case and
testable on machines without HSMs.

### Public-claim discipline (binding)

The kaspa-pq Phase 13 remote-signer claim, verbatim:

- ✅ "Remote-signer protocol decouples key custody from the
  validator process via a length-prefixed Borsh protocol over
  a Unix domain socket."
- ✅ "Equivocation guard can be relocated from the validator
  client to the signer via `SignerPolicy::Strict`, making the
  signer the single point of authority."
- ✅ "Multiple validator clients pointing at one signer is
  safe under `SignerPolicy::Strict`."
- ✅ "Audit log is BLAKE2b-512-chained for tamper detection."
- ✅ "HSM backends are pluggable via the internal
  `SignerBackend` trait; the wire protocol is HSM-agnostic."
- ❌ "Network-distributed remote signing." **Not claimed.**
  v1 is same-host Unix socket only.
- ❌ "Automatic HSM detection or zero-configuration HSM
  support." **Not claimed.** Each HSM vendor needs an explicit
  per-deployment configuration step.
- ❌ "Signer protects against malicious validator clients."
  **Not claimed.** A validator client that submits valid sign
  requests cannot be distinguished from a malicious one at the
  request level; the policy engine constrains what the signer
  agrees to do but cannot vet the intent behind a single
  well-formed request. `Strict` policy converts most attacks
  into refusals, but a malicious client that submits exactly
  one consistent attestation per epoch will get exactly one
  signature per epoch.

External material **must** use the phrasings above and **must
not** describe the protocol as "HSM-mandatory", "audit-proof",
or "network-distributable".

## Consequences

### Positive

- **Key custody decoupling.** The validator key never lives in
  the validator process's address space; even a full validator
  compromise cannot exfiltrate it.
- **HSM integration is finally possible.** Operators with
  compliance requirements (FIPS-validated key storage, NIST
  SP 800-something custody chains) can use HSM-backed signers
  without forking the validator code.
- **Equivocation guard at the right layer.** `SignerPolicy::Strict`
  moves the equivocation guard from a validator-client-local
  check (which can be by-passed by running multiple validator
  clients) to a signer-side check (which is the single
  authority that can produce a signature with the key).
- **One protocol for all signing.** Transaction, attestation,
  and takeover-token signing all go through the same wire
  format; operators do not maintain three different signing
  pipelines.
- **Audit log is first-class.** Every signing decision is
  recorded with full context, chained for integrity, and
  separable from the signer's runtime state — auditors can
  walk a frozen log without disturbing the live signer.

### Negative

- **Operational complexity.** Operators using remote-signer
  mode run an extra process (`kaspa-pq-signer`), manage its
  config, back up its audit log, and (if Strict) its
  `SignedEpochRecord` store. Operators who do not need this
  can keep using the ADR-0010 local-key-file mode (this ADR
  does not deprecate it).
- **Local-socket-only in v1.** Operators wanting a true
  air-gapped signer (different host, network-mediated) must
  wait for the follow-up ADR or use out-of-band signing (which
  defeats the validator's per-epoch responsiveness).
- **Audit log size.** A high-attendance validator on a
  fast-epoch network signs many times per day; the audit log
  grows. Compaction is out of scope here and is an operator
  problem.
- **HSM-vendor lock-in possible.** PKCS#11 is portable in
  theory but vendor-specific in practice. Operators choosing
  an HSM commit to its quirks until they migrate keys (which
  requires a new bond — there is no "re-key" path for an
  active validator at the consensus layer).

### Neutral

- **Local-key-file mode unchanged.** ADR-0010's local-key
  validator still works; this ADR is opt-in. Operators with
  no HSM requirement can ignore the remote-signer surface
  entirely.
- **No on-chain surface.** The remote-signer protocol is
  node-local; consensus does not know the signer exists. The
  artefacts it produces (signatures) are indistinguishable
  from locally-produced ones at the chain level.

## Phase 13 PR plan (this ADR's slot)

| PR | Title | Status |
|---|---|---|
| 13.9 | This ADR | landed |
| 13.10 | `dns_finality.rs` `SignerProtocolVersion` + `SignerCapabilities` + `SignerHello{,Ack}` + `SignerRequest` / `SignerResponse` + `SigningPurpose` + `SignerMetadata` + `SignerError` + `SignerPolicy` + `SignerAuditRecord` + `compute_signer_audit_chain_entry` helper + tests | next |
| 13.11 | Spec update (ADR-0014 + ADR-0015 + Phase 13 row 4/4 + v0.8 — closes Phase 13) | after PR-13.10 |

Implementation slots, gated on Phase 1–9 baseline + PR-10.6
(validator service):

| PR | Title | Layers onto | Status |
|---|---|---|---|
| 10.6′′′′ | `kaspa-pq-signer` binary, default `SoftwareKey` backend; `--policy {permissive,auditonly,strict}` flag | PR-10.6 (validator service) | deferred |
| 10.6′′′′a | `--signer-socket <path>` flag on `kaspa-pq-validator`; protocol handshake; sign-request fan-out | PR-10.6 + PR-10.6′ (sidecar) | deferred |
| 10.12′′ | Strict-mode signer-side equivocation guard via `check_signed_epoch_record` integration | PR-10.12 + this ADR | deferred |
| 10.12′′a | `Pkcs11Adapter` build feature for the signer (per-vendor configuration documented per-deployment) | PR-10.12 | deferred |

## References

- [ADR-0010 — Validator Node Architecture](0010-validator-node-architecture.md)
  §"Negative" (the "key management on a hot node" future-ADR
  pointer this ADR resolves).
- [ADR-0011 — Validator Single-Host Deployment + Equivocation-Safety](0011-validator-deployment-and-equivocation-safety.md)
  (the `SignedEpochRecord` + `check_signed_epoch_record` guard
  whose authoritative location moves to the signer under
  `SignerPolicy::Strict`).
- [ADR-0014 — Coordinated-Failover Protocol](0014-coordinated-failover-protocol.md)
  (TakeoverToken is one of the message types the signer can be
  asked to sign — covered by `SigningPurpose::TakeoverToken`).
- ADR-0017 (future, candidate) — Network-distributed remote
  signing. Adds TLS / mTLS and a clear partition policy for
  validator ↔ signer connections crossing a network boundary;
  out of scope for this ADR's v1.
