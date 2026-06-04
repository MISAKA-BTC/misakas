# ADR-0014: Coordinated-Failover Protocol for Validator Hosts

> **⚠️ ML-DSA-65-era design doc (HISTORICAL — audit M-03).** The signature scheme is now **ML-DSA-87** (pk 2592 B / sig 4627 B) per [ADR-0019](0019-mldsa87-migration.md); the `ML-DSA-65` / `1952` / `3309` values below are the original draft and are **not current consensus**. This failover protocol is deferred/unwired roadmap (see audit H-03).

Status: Accepted (Phase 13 design freeze; implementation deferred to Phase 10 PR series)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0002](0002-mldsa65-p2pkh.md) (ML-DSA-65 signature
            scheme used for the TakeoverToken),
            [ADR-0008](0008-hash64-consensus-identity.md) (Hash64
            for validator identifiers),
            [ADR-0009](0009-dns-probabilistic-finality.md)
            (SlashingEvidencePayload — the consensus safety net
            this ADR's local protocol stays well clear of),
            [ADR-0010](0010-validator-node-architecture.md)
            (validator service runtime; the new
            AwaitingTakeoverToken status integrates into this
            loop),
            [ADR-0011](0011-validator-deployment-and-equivocation-safety.md)
            ("one key, one host" invariant this ADR refines).

## Context

[ADR-0011 §"Hardening checklist"](0011-validator-deployment-and-equivocation-safety.md)
states:

> Same validator key MUST NOT run on a second host concurrently.
> ...
> A future ADR may add a coordinated-failover protocol; until
> then, 'hot/cold' failover is operator-managed and bound to the
> 'one key, one host' invariant.

This ADR is that future ADR.

Operators running validators on a single host face an
availability trade-off:
- One host → no equivocation risk, but the validator goes
  offline whenever the host is rebooted, fails, or undergoes
  hardware maintenance. Per ADR-0013 §"Bond ROI economics"
  downtime costs rewards but not principal.
- Two hosts (hot/standby) → continuous availability, but if
  both ever sign different attestations for the same epoch the
  bond is slashed (ADR-0009 §"SlashingEvidencePayload").

ADR-0011's "operator-managed" wording is the right baseline
(operators today use cold spares + manual restart procedures),
but it leaves a class of operator who wants automated failover
without a meaningful safety guarantee. The result on other
networks has historically been catastrophic slashing events
caused by mis-configured failover pairs.

This ADR specifies a **minimum viable coordinated-failover
protocol**: a local-only, signature-bound `TakeoverToken` that
explicitly transfers signing authority from one host to another
at a specific future epoch. The protocol is honest-operator
oriented (the slashing rule in ADR-0009 remains the real safety
net for malicious operators); its goal is to make accidental
double-signing across a planned handoff impossible.

Two alternatives considered and **explicitly out of scope** for
this ADR:

| Alternative | Why deferred |
|---|---|
| **Shared signed-epoch DB** (both hosts write to a single sled / RocksDB over NFS or a clustered filesystem) | Cryptographically weakest of the three; depends on distributed-filesystem correctness for safety. Operationally fragile (NFS lock storms, split-brain). A future ADR may add it as an opt-in lower-friction alternative for operators who already run reliable shared storage. |
| **Threshold / multi-party signing (FROST-style)** | Cryptographically strongest; the validator key is split into two shares, each host holds one, signing requires both. **Not available** for ML-DSA-65 today — there is no standardised post-quantum threshold-signature scheme that composes with ADR-0002's libcrux `sign_ctx` API. A follow-up ADR (candidate ADR-0016) can add this once a PQ-TSS standard exists. |

The TakeoverToken protocol specified here is the **minimum
viable** middle ground: cryptographically meaningful (an
ML-DSA-65 signature by the validator key, so it cannot be
forged), operationally simple (a single file copied between
hosts out-of-band), and forward-compatible with both
alternatives above.

## Decision

### Two-host hot/standby topology (the only topology this ADR covers)

```text
host A (primary)                     host B (standby)
┌──────────────────────────┐         ┌──────────────────────────┐
│ kaspa-pq-node            │         │ kaspa-pq-node            │
│ kaspa-pq-validator       │         │ kaspa-pq-validator       │
│   --signed-epoch-db /…   │         │   --signed-epoch-db /…   │
│   (currently signing)    │         │   (status:               │
│                          │         │    AwaitingTakeoverToken)│
│ /etc/kaspa-pq/           │         │ /etc/kaspa-pq/           │
│   validator.mldsa        │         │   validator.mldsa        │
│   (same key file)        │         │   (same key file —       │
│                          │         │    bit-for-bit identical)│
└──────────────────────────┘         └──────────────────────────┘
        │                                        ▲
        │  TakeoverToken                         │
        │  (out-of-band file copy: scp, file     │
        │   share, encrypted email — operator    │
        │   choice; the token is signed so the   │
        │   transport need not be authenticated) │
        └────────────────────────────────────────┘
```

Both hosts:
- Run a full node + validator service per ADR-0011.
- Hold an **identical** ML-DSA-65 validator key file. The key is
  copied **once** at standby provisioning time, then never
  again.
- Maintain **independent** signed-epoch DBs (no shared
  filesystem). Each host's DB reflects what that host has
  signed.
- Are configured with each other's `host_id` (see below) so
  they recognise tokens from one another.

This ADR does **not** cover:
- Three-or-more host topologies (cascading failover; out of
  scope, would require a totally-ordered linear chain of tokens
  or a quorum mechanism);
- Remote-validator topologies where hosts are on different
  networks (ADR-0011 §"Same host" invariant forbids; this ADR
  inherits that — both hosts must be in the same fault domain);
- Live-load-balancing (both hosts active at once); this would
  defeat the protocol's safety property.

### `host_id` derivation

Each host computes a stable identifier at startup:

```text
host_id = BLAKE2b-256(
    key   = HOST_ID_KEY,
    input = hostname.as_bytes() || host_boot_nonce.as_bytes(),
)

HOST_ID_KEY = b"kaspa-pq-validator-host-id-v1"
```

Where `host_boot_nonce` is a fresh 32-byte random generated by
`kaspa-pq-cli validator host-id init` and persisted at
`/etc/kaspa-pq/host-nonce`. The nonce makes `host_id` rebuild-
stable but resistant to spoofing — an operator who rebuilds the
secondary host gets a new `host_id` unless they explicitly
re-use the nonce file.

`host_id` is the upstream 32-byte `Hash` (alias `Hash32`); it
does **not** need 64 bytes because it never enters consensus
state — it is only used inside `TakeoverToken` and the local
status surface.

```text
HOST_ID_KEY = b"kaspa-pq-validator-host-id-v1"
```

Consensus-irrelevant (this is a node-local protocol), but bumped
on the same `-v1` discipline as other domain keys so renaming
later is auditable.

### `TakeoverToken`

```rust
pub struct TakeoverToken {
    pub version: u16,

    /// host_id of the validator currently signing (the yielding
    /// side). Must match the host that generated the token.
    pub yielding_host_id: Hash,

    /// host_id of the validator about to start signing (the
    /// taking-over side). The receiving host MUST refuse to
    /// honor a token whose taking_over_host_id ≠ its own
    /// host_id.
    pub taking_over_host_id: Hash,

    /// Validator identity both hosts share. Must match the
    /// receiving host's --stake-bond → validator_id.
    pub validator_id: Hash64,

    /// First epoch at which the taking-over host may sign. The
    /// yielding host MUST NOT sign any epoch ≥ valid_from_epoch
    /// after issuing this token.
    pub valid_from_epoch: u64,

    /// Number of epochs of grace overlap during which neither
    /// host signs (defensive against in-flight gossip).
    /// Typically 1; max 8 (one epoch ≈ minutes, anything longer
    /// is a configuration error). The taking-over host starts
    /// signing at `valid_from_epoch + grace_epochs`.
    pub grace_epochs: u8,

    /// Wall-clock issuance timestamp (informational; not part
    /// of the signed material — clocks drift, so consensus and
    /// the protocol do not rely on it).
    pub issued_at_unix_secs: u64,

    /// 3309-byte ML-DSA-65 signature by the validator key over
    /// `takeover_token_message(...)` (see helper below) with
    /// `TAKEOVER_TOKEN_CONTEXT` as the libcrux `ctx` parameter.
    pub signature: Vec<u8>,
}
```

The token's signed message:

```text
takeover_token_message(yielding, taking_over, validator_id,
                       valid_from_epoch, grace_epochs) =
    BLAKE2b-256(
        key   = TAKEOVER_TOKEN_MESSAGE_DOMAIN,
        input = yielding.as_bytes()              (32 B)
             || taking_over.as_bytes()           (32 B)
             || validator_id.as_bytes()          (64 B)
             || valid_from_epoch.to_le_bytes()    (8 B)
             || [grace_epochs]                    (1 B),
    )
```

Domain keys (consensus-irrelevant but `-v1`-disciplined for
auditability):

```text
TAKEOVER_TOKEN_MESSAGE_DOMAIN = b"kaspa-pq-takeover-token-v1"
TAKEOVER_TOKEN_CONTEXT        = b"kaspa-pq-v1/takeover/mldsa65"
```

The context is **distinct** from both the transaction context
(`b"kaspa-pq-v1/tx/mldsa65"`, ADR-0002) and the attestation
context (`b"kaspa-pq-v1/att/mldsa65"`, ADR-0009 §"Attestation
target"), so a takeover-token signature can never be replayed
as a transaction or attestation signature, and vice versa. This
keeps the same replay-safety property the ADR-0009 split
already enforces.

### Handoff protocol (planned failover)

```text
1.  Operator on host A (primary):
        kaspa-pq-cli validator handoff \
            --to <host_B_id> \
            --valid-from-epoch <current_epoch + 2> \
            --grace-epochs 1 \
            --out /tmp/takeover.tkn

    Host A's validator service:
      a. Asserts its signed-epoch DB has NOT signed
         valid_from_epoch yet.
      b. Sets a local "yielded-at" sentinel = (validator_id,
         valid_from_epoch) so it will refuse to sign any epoch
         ≥ valid_from_epoch from now on.
      c. ML-DSA-65 signs takeover_token_message and emits the
         TakeoverToken file.

2.  Operator transfers /tmp/takeover.tkn to host B (scp / file
    share / encrypted email — token signature self-authenticates,
    so the transport need not be authenticated).

3.  Operator on host B (standby):
        kaspa-pq-cli validator accept-takeover \
            --token /tmp/takeover.tkn

    Host B's validator service:
      a. Verifies signature against the validator pubkey from
         its --stake-bond.
      b. Asserts taking_over_host_id == its own host_id.
      c. Asserts validator_id matches its --stake-bond.
      d. Asserts valid_from_epoch > current epoch (a stale
         token is rejected).
      e. Stores the token in local DB keyed by
         (validator_id, valid_from_epoch).
      f. Transitions ValidatorStatus from
         AwaitingTakeoverToken → ActiveIdle.

4.  At epoch (valid_from_epoch + grace_epochs), host B starts
    signing.

5.  Host A's validator service refuses to sign any epoch ≥
    valid_from_epoch. Operator may shut it down or leave it
    running as a cold spare.
```

To hand back, the same protocol runs in the opposite direction
with a new token signed by the same validator key, this time
yielding from B to A.

### Failure modes

| Failure | Consequence |
|---|---|
| Host A crashes before emitting the token | No automatic failover. Operator must accept downtime until A recovers, or use the **emergency handoff** path (see below). The validator key is on host B, but B will not sign without a token. |
| Token transport fails (lost file) | Operator re-runs `handoff` to generate a new token with the same `(yielding, taking_over, validator_id, valid_from_epoch)` — the token is deterministic-up-to-the-signature (which is hedged) so re-issuing is safe. |
| Network split between A and B with B holding a valid token | Both hosts have the same key, but the token's `valid_from_epoch` is the consensus point: A refuses to sign at ≥ `valid_from_epoch`, B refuses to sign at < `valid_from_epoch`. The chain's view of who-signed-what comes from on-chain attestations, not from inter-host coordination. |
| Operator types `--force` on host A to override the yielded-at sentinel | Honest-operator self-harm. Host A signs at `valid_from_epoch`, host B signs at the same epoch (both have the same key and the same `target_hash` after the chain finalises), the resulting `SlashingEvidencePayload` burns the bond. ADR-0009 consensus slashing is the real safety net; this ADR cannot prevent it. |
| Host B's `accept-takeover` is replayed against a different host | Token's `taking_over_host_id` field binds it to a specific host; replay against any other host fails the check at step 3.b. |

### Emergency handoff (without a token)

Operators occasionally need to take over from a crashed host
that never emitted a token. This ADR defines the
**slashing-acknowledged** emergency path:

```bash
kaspa-pq-cli validator emergency-takeover \
    --acknowledge-slashing-risk \
    --previous-host-last-known-epoch <N>
```

Behaviour:
- Host B starts signing at epoch `N + 1`.
- If host A is in fact still alive and signed any epoch ≥
  `N + 1`, the chain sees both hosts' attestations and the
  bond is slashed.
- The `--acknowledge-slashing-risk` flag is a barrier-of-entry
  to prevent accidental invocation; it does **not** alter
  consensus.

This path is for operator emergencies (host A is in flames /
unrecoverable / hardware-stolen) and is `unsafe` in the same
sense that "rm -rf with sudo" is `unsafe` — useful with
discipline, catastrophic without. The protocol does not try to
make it safe; it just makes it explicit and audit-loggable.

### Local-storage layout

The local DB on each host gains one new table:

```text
~/.kaspa-pq/takeover-tokens/
    held_by_us/                # tokens this host is the recipient of
        <validator_id_hex>_<valid_from_epoch>.tkn
    issued_by_us/              # tokens this host issued (yielding side)
        <validator_id_hex>_<valid_from_epoch>.tkn
    yielded-at.json            # local sentinel: { validator_id, valid_from_epoch }
                               # set when we issued a token; refuse to sign
                               # any epoch ≥ valid_from_epoch.
```

Backup of `held_by_us/` is critical (loss = inability to fail
over). `issued_by_us/` is informational. `yielded-at.json` is
critical (loss = honest operator could re-sign at the yielded
epoch and self-slash).

### `ValidatorStatus` extension

A new variant is appended to the [ADR-0011 §"Validator status
enum"](0011-validator-deployment-and-equivocation-safety.md):

```rust
pub enum ValidatorStatus {
    NodeNotSynced     = 0,
    BondNotFound      = 1,
    BondPending       = 2,
    ActiveIdle        = 3,
    ActiveEligible    = 4,
    SignedThisEpoch   = 5,
    Unbonding         = 6,
    Slashed           = 7,
    DryRun            = 8,
    // NEW in PR-13.8 per this ADR:
    AwaitingTakeoverToken = 9,
}
```

`AwaitingTakeoverToken` is set on a standby host that has booted
with `--enable-validator` and `--stake-bond …` but has not yet
received a valid TakeoverToken for any future epoch. The
existing discriminant pin (0..8 already API-stable per ADR-0011)
is preserved — variant 9 is **appended**, not inserted, so RPC
clients that haven't been updated still parse the older nine
variants identically.

### Public-claim discipline (binding)

The kaspa-pq Phase 13 coordinated-failover claim, verbatim:

- ✅ "Coordinated planned failover between two same-host
  validator hosts via a cryptographically-signed
  `TakeoverToken`."
- ✅ "Honest-operator double-signing across a planned handoff
  is prevented by the yielding-side `yielded-at` sentinel + the
  taking-over-side token requirement."
- ✅ "Replay-safe across protocol surfaces: the takeover-token
  signing context is distinct from both the transaction and
  attestation contexts."
- ❌ "Tolerates crashed-primary failover without slashing risk."
  **Not claimed.** The emergency-takeover path is explicitly
  slashing-acknowledged.
- ❌ "Tolerates malicious secondary." **Not claimed.** Both
  hosts hold the same validator key; a malicious secondary can
  always equivocate directly. Consensus slashing
  (`SlashingEvidencePayload`, ADR-0009) is the real safety
  guarantee — this protocol is honest-operator UX.
- ❌ "Live load-balanced two-active configuration." **Not
  claimed and not supported.** The protocol is strictly
  hot/standby; running both hosts active at once defeats the
  safety property.

External material **must** use the phrasings above and **must
not** describe the protocol as "active/active", "TSS-equivalent",
or "slashing-proof".

## Consequences

### Positive

- **Planned handoff is safe.** An operator who runs the
  documented handoff workflow cannot accidentally double-sign;
  the `yielded-at` sentinel plus the taking-over-side token
  check makes accidental concurrent signing impossible without
  explicit `--force`.
- **No shared filesystem dependency.** Each host's local DB is
  independent; the protocol works over any out-of-band
  transport (scp, encrypted email, USB key).
- **Replay-safe.** A captured token cannot be replayed against
  any host other than `taking_over_host_id`, and the signature
  context is distinct from every other ML-DSA-65 use site.
- **Honest-operator-tested in CI.** The protocol is fully
  exercised by the PR-10.14′ simnet smoke test (acceptance
  criteria in §11 of `kaspa-pq-spec.md` Phase 13 acceptance
  4/4 once landed).
- **`ValidatorStatus` extension is additive.** Existing nine
  variants keep their discriminants; the new
  `AwaitingTakeoverToken` is appended. RPC clients that
  haven't been updated still parse what they could before.

### Negative

- **Both hosts hold the validator key.** The protocol does not
  reduce the surface area of key compromise — if either host
  is breached, the validator key is gone. Operators wanting
  key-compromise resistance need the future ADR-0016 TSS
  variant.
- **Manual operator step.** Each handoff requires running two
  CLI commands and transferring a file between hosts. No
  automated heartbeat-based failover; that would require the
  shared-DB or TSS variants.
- **No automatic recovery from primary crash.** Slashing-
  acknowledged emergency path exists but is explicitly
  `unsafe` and operator-acknowledged.
- **Asymmetric backup criticality.** `held_by_us/` and
  `yielded-at.json` MUST be in backups; `issued_by_us/` is
  informational. Operators new to the protocol may underestimate
  the criticality of `yielded-at.json` (its loss is the path to
  self-slashing).

### Neutral

- **Same-host invariant unchanged.** Both hosts must still be
  in the same fault domain per ADR-0011 §"Same host". This ADR
  does not unlock remote-host failover.
- **No on-chain surface.** Takeover tokens are node-local
  artifacts. Consensus is not aware they exist; consensus only
  sees the attestations themselves and the equivocation
  evidence (if any).

## Phase 13 PR plan (this ADR's slot)

| PR | Title | Status |
|---|---|---|
| 13.7 | This ADR | landed |
| 13.8 | `dns_finality.rs` `HostId` alias + `TakeoverToken` + `takeover_token_message` helper + `verify_takeover_token` helper + `ValidatorStatus::AwaitingTakeoverToken` variant + tests | next |
| 13.11 | Spec update (ADR-0014 + ADR-0015 + Phase 13 row 4/4 + v0.8 — closes Phase 13) | after PR-13.9 / PR-13.10 |

Implementation slots, gated on Phase 1–9 baseline + PR-10.6′
(sidecar binary):

| PR | Title | Layers onto | Status |
|---|---|---|---|
| 10.6′′′ | `kaspa-pq-cli validator host-id init` + `validator handoff` + `validator accept-takeover` + `validator emergency-takeover --acknowledge-slashing-risk` CLI commands; local `takeover-tokens/` DB layout | PR-10.6′ (sidecar binary) | deferred |
| 10.14′′ | TakeoverToken-driven handoff smoke test on simnet (two-host topology, simulated planned + emergency paths) | PR-10.14′ (sidecar smoke) | deferred |

## References

- [ADR-0009 — DNS Probabilistic Finality Overlay](0009-dns-probabilistic-finality.md)
  §"`SlashingEvidencePayload`" (the consensus-side safety net
  this protocol stays well clear of).
- [ADR-0010 — Validator Node Architecture](0010-validator-node-architecture.md)
  §"Validator service runtime" (the runtime loop the new
  `AwaitingTakeoverToken` state integrates into).
- [ADR-0011 — Validator Single-Host Deployment + Equivocation-Safety](0011-validator-deployment-and-equivocation-safety.md)
  §"Hardening checklist" (the "one key, one host" invariant
  this ADR refines into "one key, one *signing* host, with
  signed-handoff").
- ADR-0016 (future, candidate) — Threshold-signing failover.
  PQ-TSS scheme dependent; will replace this ADR for
  operators wanting key-compromise-resistant failover once
  a standardised PQ-TSS construction lands.
