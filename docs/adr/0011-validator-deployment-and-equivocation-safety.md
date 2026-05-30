# ADR-0011: Validator Single-Host Deployment + Equivocation-Safety Operating Model

Status: Accepted (Phase 12 design freeze; implementation deferred to Phase 10 PR series)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0001](0001-network-isolation.md) (kaspa-pq is a new
            network), [ADR-0002](0002-mldsa65-p2pkh.md) (ML-DSA-65
            signing scheme), [ADR-0008](0008-hash64-consensus-identity.md)
            (Hash64 identifiers), [ADR-0009](0009-dns-probabilistic-finality.md)
            (DNS overlay consensus rules + slashing payload),
            [ADR-0010](0010-validator-node-architecture.md) (node-role
            separation + in-process validator service).
Relation to ADR-0010: operational supplement. Where this ADR and
ADR-0010 overlap (single-binary integrated mode), ADR-0010 is
authoritative. Where this ADR introduces new surface (sidecar
binary, signed-epoch DB, `ValidatorStatus` enum, slashing-scope
binding, key-separation policy), this ADR is authoritative.

## Context

ADR-0010 codified the in-process validator service spawned by
`kaspa-pq-node` when `--enable-validator` is set. That model is
correct but it is **one** of two valid deployment shapes;
production operators almost always also want the **sidecar**
shape (separate process, separate restart cadence, separate log
stream, separate dry-run capability, separate failure blast
radius).

ADR-0010 is silent on:

- The sidecar deployment shape itself.
- Persistent equivocation safety: the validator service must
  remember what it has signed across restarts so an honest
  operator cannot accidentally double-sign across a node restart.
  ADR-0009 §"`SlashingEvidencePayload`" defines the consensus
  evidence; ADR-0010 mentions the local guard in passing
  (`~/.kaspa-pq/validator-state.json`) but does not specify the
  record shape or the allow / re-broadcast / block decision rule.
- Key separation between the **validator signing key** (hot, on
  the validator host) and the **owner key** (cold, off the
  validator host, used for `StakeBond` / `StakeUnbond` /
  withdrawal). ADR-0010 mentions key management on a hot node as
  a Negative; this ADR turns that into a binding policy.
- The boundary between slashable equivocation and non-slashable
  downtime. ADR-0009 §"`SlashingEvidencePayload`" defines only
  the equivocation case; this ADR makes the downtime exemption
  explicit so a future PR cannot quietly add "missed-epoch
  slashing" without its own ADR.
- The status surface a `validator status` RPC must return so an
  operator can answer "is my bond healthy?" without reading the
  source code.

This ADR fills all of those gaps.

## Decision

### Two supported deployment shapes

| Shape | Process layout | When to use |
|---|---|---|
| **Integrated** (ADR-0010) | One `kaspa-pq-node` process with `--enable-validator …` flags | Simplest path; single-operator small validator; CI/simnet. |
| **Sidecar** (this ADR, **recommended for production**) | Two processes on the same host: `kaspa-pq-node` + `kaspa-pq-validator`, the latter connecting via 127.0.0.1 wRPC | Default for production. Restart the node without restarting the validator (and vice versa); key isolation by file mode; smaller blast radius if a validator-loop bug surfaces; per-process logs; per-process resource limits. |

Invariants common to both shapes (lifted from ADR-0010 +
strengthened):

- **Same host.** The validator service must run on the same host
  as its full node. ADR-0010 §"Invariants" binds this; this ADR
  inherits it. A "remote validator over the public internet" is
  out of scope and would require a follow-up ADR.
- **Local-only RPC.** The validator → node connection binds to
  `127.0.0.1` (or a Unix-domain socket in a future ADR) and is
  **not** exposed to the network. Operators must firewall the
  node's RPC port on all public interfaces.
- **Same network.** The validator refuses to start if the node
  it connects to reports a different `network_id`. Mis-pointing a
  mainnet validator at a testnet node is a startup-time failure,
  not a silent mis-attest.
- **One key, one host.** The same validator key MUST NOT run on a
  second host concurrently. Misconfigured failover pairs are the
  single biggest equivocation risk; the binary refuses to start
  if an attestation that does **not** match the local
  signed-epoch DB is observed gossiping under the local
  `validator_id` (see §"Signed-epoch persistence" below).

### Sidecar binary: `kaspa-pq-validator`

New binary, ships next to `kaspa-pq-node` in `kaspad/`. CLI shape:

```text
kaspa-pq-validator \
    --node-rpc 127.0.0.1:<port>          # local-only
    --validator-key <path>
    --stake-bond <bond_outpoint_hex>
    [--validator-mode {offchain|shard|full}]
    [--max-attestations-per-block <u16>]
    [--dry-run]                          # log/calculate only; do not sign
    [--signed-epoch-db <path>]           # default ~/.kaspa-pq/validator-state.db
```

Notes:

- A `--dry-run` validator computes the per-epoch eligibility, the
  attestation target, and the would-be attestation message; it
  does **not** call libcrux's ML-DSA-65 `sign_ctx` and does
  **not** gossip. Useful for new operators verifying their bond
  before going live, and for smoke tests on simnet.
- `--signed-epoch-db` defaults to a local on-disk store
  (sled / RocksDB). Backup of this file is a deployment best
  practice; loss of the file does not lose the bond but loses
  the local equivocation guard. The on-chain
  `SlashingEvidencePayload` is the consensus guarantee — the
  local guard exists to prevent honest accidents across restarts,
  not to protect a malicious operator from themselves.
- The same `--enable-validator …` flag set on `kaspa-pq-node`
  keeps working (ADR-0010 backward-compatibility). The sidecar
  shape is an opt-in alternative, not a deprecation.

### Validator status enum (`ValidatorStatus`)

Returned by both the in-process service and the sidecar through
`getValidatorStatus` RPC + `kaspa-pq-cli validator status`:

| Variant | Meaning |
|---|---|
| `NodeNotSynced` | Local node has not yet reached `is_synced()`. Validator service stays idle. |
| `BondNotFound` | `--stake-bond` outpoint does not exist in the stake registry yet. |
| `BondPending` | Bond exists, `daa_score < activation_daa_score`. |
| `ActiveIdle` | Bond is active; current epoch validator-set sortition has not yet picked this validator. |
| `ActiveEligible` | Bond is active, validator is in the current epoch's set, and `signed_epoch_db` shows no prior signature for this epoch. |
| `SignedThisEpoch` | Already signed the current epoch — recorded in `signed_epoch_db`. |
| `Unbonding` | Bond is in the unbonding window. |
| `Slashed` | Bond has been burned by a `SlashingEvidencePayload`. The validator service exits with a non-zero status. |
| `DryRun` | `--dry-run` set; per-epoch computation runs, signing is skipped. |

Total: nine variants. The CLI / RPC surface returns the variant
plus contextual fields (current epoch, last signed epoch,
attestations gossiped count, attestations included on-chain
count, missed-epochs count, free-text reason).

Default variant is `NodeNotSynced` — a freshly-started validator
is "not yet sure if the node it just connected to is at tip".
This conservative default keeps an in-flight `Allow` decision
from being made before the runtime loop's first poll.

### Signed-epoch persistence (`SignedEpochRecord`)

Per ADR-0009 §"`SlashingEvidencePayload`", an attestation pair is
slashable evidence iff the two records share
`(bond_outpoint, validator_id, epoch)` **and** differ on
`(target_hash | target_daa_score)`. The validator service must
therefore persist what it has signed across restarts:

```rust
pub struct SignedEpochRecord {
    /// Epoch this attestation is bound to.
    pub epoch: u64,
    /// Selected-chain anchor the attestation approves.
    pub target_hash: Hash64,
    pub target_daa_score: u64,
    /// BLAKE2b-512 of the 3309-byte ML-DSA-65 signature. Pinned
    /// so a re-broadcast of an in-flight attestation is
    /// detectable / loggable without re-storing the full
    /// signature blob.
    pub signature_fingerprint: Hash64,
}
```

Before issuing a new attestation, the validator checks the
candidate against the prior record for the same
`(bond_outpoint, validator_id, epoch)` triple, returning one of
three outcomes:

```text
check_signed_epoch_record(prev, candidate):
    match prev {
        None                                         => Allow,
        Some(p) if p.target_hash == c.target_hash &&
                   p.target_daa_score == c.target_daa_score
                                                     => AllowRebroadcast,
        Some(_)                                      => Block,
    }
```

- `Allow` — never signed this epoch; sign and gossip.
- `AllowRebroadcast` — already signed the exact same target this
  epoch (e.g. node restarted mid-gossip). Re-sending the same
  attestation is **not** equivocation; the validator service may
  re-gossip but is not required to.
- `Block` — would be equivocation if sent. The validator service
  refuses to sign and surfaces the conflict in logs and the
  status RPC.

Note: `signature_fingerprint` is stored for forensics and
log-grepping, **not** for the equivocation decision. ML-DSA-65 is
hedged by default (FIPS 204 §3.4) and two valid signatures over
the same message will differ on the `rnd` parameter; therefore
signature-bit equality is too strict to be the safety predicate.
Target-hash + target-daa-score equality is the right predicate.

### Key separation policy (binding)

| Key | Role | Storage |
|---|---|---|
| `validator_key` (ML-DSA-65) | Sign attestations | On the validator host. `chmod 600`, owned by a dedicated `kaspa` user. |
| `owner_key` (ML-DSA-65) | Sign `StakeBond` / `StakeUnbond` / withdrawal txs | **Not** on the validator host. Cold storage or a separate operator workstation. |

A leaked validator key risks slashing if the attacker can
double-sign on the same `(bond_outpoint, validator_id, epoch)`
triple as the honest operator; an `owner_key` leak is much worse
(it can withdraw the bond outright). The split is enforced by
wallet UX: `kaspa-pq-cli validator keygen --out …` only emits the
validator key; the owner key material is produced by
`kaspa-pq-cli wallet …` and never copied to the validator host by
any first-party tool.

### Slashing scope (binding)

| Condition | Consensus consequence |
|---|---|
| Signs two incompatible attestations at `(bond_outpoint, validator_id, epoch)` | **Slashed** — full bond burned per ADR-0009 §"`SlashingEvidencePayload`". |
| Goes offline / misses one or more epochs | **Not slashed.** Reward loss only (future PR-10.5 / PR-10.15 reward distribution). |
| Equivocates without a counterpart attestation actually published (e.g. signed twice in `--dry-run`) | **Not slashable** — no on-chain evidence. The local guard is honest-operator UX, not the consensus safety property. |

The downtime exemption is **binding**: a Phase 10 PR that adds
"missed-epoch slashing" requires its own follow-up ADR. The
rationale is that the DNS overlay tolerates partial validator
liveness by design (`StakeScore` aggregates whatever subset
signed); the failure mode of a one-host validator restarting is
reward loss, not bond loss. Penalising downtime would push
operators toward fragile hot-standby topologies that **increase**
equivocation risk — exactly the wrong trade.

### Auto-startup ordering

A sidecar `kaspa-pq-validator` can be started **before** its bond
is active. The runtime loop tolerates every "not yet" state:

```text
loop {
    match validator_state() {
        NodeNotSynced     => sleep(5s);   continue;
        BondNotFound      => sleep(30s);  continue;   // tx may still be propagating
        BondPending       => sleep(60s);  continue;   // wait for activation_daa_score
        Unbonding         => warn("unbonding; will exit when finalised"); sleep(60s); continue;
        Slashed           => return Err(FatalSlashed);
        DryRun            => compute_and_log(); sleep(epoch_length); continue;
        ActiveIdle        => sleep(min(epoch_length / 10, 30s)); continue;
        ActiveEligible    => sign_and_gossip()?;
        SignedThisEpoch   => sleep(until_next_epoch); continue;
    }
}
```

Operationally this means an operator can:

```bash
# Step A: start the node
systemctl start kaspa-pq-node

# Step B: start the validator service in advance (it will wait)
systemctl start kaspa-pq-validator

# Step C: at any later time, submit the bond
kaspa-pq-cli stake bond …

# Step D: at activation_daa_score, the validator service starts
# signing automatically — no manual restart needed
```

The order `(B, C)` is interchangeable. The operator can also
submit the bond first and start the validator service afterward;
both directions are supported by the state machine.

### Hardware sizing (informative)

Full node + validator on one host, **no** PoW mining:

| Resource | Minimum (testnet / hobby) | Recommended (mainnet) |
|---|---|---|
| CPU | 4 vCPU | 8 vCPU |
| RAM | 8 GB | 16 GB+ |
| Storage | 500 GB NVMe | 1 TB+ NVMe |
| Network | persistent, high upload | symmetric gigabit |
| OS | Linux + systemd | Linux + systemd |

Hash64 + ML-DSA verify + StakeShard verify + LtHash add to the
per-block CPU and disk cost relative to upstream Kaspa.
Validator on a small VPS is viable for testnet; for mainnet the
recommended row is the deployment floor.

Co-locating PoW mining on the same host is **explicitly not
supported** by this ADR: Phase 2+ memory-hard `algo_id` variants
will defeat small-VPS CPUs, and even at `algo_id = 1` the
contention with consensus validation hurts both jobs. Operators
who want to mine should do so on a separate host.

### Systemd reference units (informative)

Sidecar deployment, two units linked by `Requires=` so the
validator follows the node lifecycle:

```ini
# /etc/systemd/system/kaspa-pq-node.service
[Unit]
Description=Kaspa-PQ Full Node
After=network-online.target
Wants=network-online.target

[Service]
User=kaspa
Group=kaspa
ExecStart=/usr/local/bin/kaspa-pq-node \
    --network kaspa-pq-mainnet \
    --datadir /var/lib/kaspa-pq \
    --rpclisten-borsh 127.0.0.1:27110
Restart=always
RestartSec=5
LimitNOFILE=1048576

[Install]
WantedBy=multi-user.target
```

```ini
# /etc/systemd/system/kaspa-pq-validator.service
[Unit]
Description=Kaspa-PQ Validator Service
After=kaspa-pq-node.service
Requires=kaspa-pq-node.service

[Service]
User=kaspa
Group=kaspa
ExecStart=/usr/local/bin/kaspa-pq-validator \
    --node-rpc 127.0.0.1:27110 \
    --validator-key /etc/kaspa-pq/validator.mldsa \
    --stake-bond <bond_outpoint_hex>
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Hardening checklist (informative; binding policy is in
§"Key separation policy" and §"Slashing scope" above):

```text
- RPC binds to 127.0.0.1 only; never bind 0.0.0.0
- P2P port public; RPC port firewalled on all public interfaces
- validator key file:  chmod 600, owned by `kaspa` user
- owner key:           NOT on this host
- SSH: key-only, no root login
- signed-epoch DB:     part of regular backups
- Restart=always on both systemd units
- Same validator key MUST NOT run on a second host concurrently
```

The last line is the single biggest equivocation risk. A
misconfigured failover pair is the textbook way to get slashed.
A future ADR may add a coordinated-failover protocol; until then,
"hot/cold" failover is operator-managed and bound to the
"one key, one host" invariant.

## Consequences

### Positive

- **Operator clarity.** Two named deployment shapes
  (integrated, sidecar) with explicit trade-offs; status surfaced
  through a single 9-variant enum; the runbook (ADR-0010 §8)
  works for both shapes with the sidecar variant adding
  one `systemctl start kaspa-pq-validator` line.
- **Restart independence.** Validator can restart without
  taking down the consensus pipeline (sidecar mode); node can
  restart without losing the validator's signed-epoch DB.
- **Honest-operator double-sign protection.** Persistent
  signed-epoch DB plus the `Allow / AllowRebroadcast / Block`
  outcome enum prevents accidental double-signing across
  restarts. Consensus slashing (ADR-0009) remains the safety net
  for malicious / coordinated double-signing.
- **Slashing surface is small and named.** Downtime is
  explicitly non-slashable; equivocation is the only slashable
  condition. Future "missed-epoch slashing" proposals require a
  follow-up ADR — they cannot land as an implementation detail.
- **Key separation is policy, not folklore.** Validator key on
  the host; owner key off the host. `kaspa-pq-cli` UX enforces
  the split; the hardening checklist makes it scannable for
  operators.

### Negative

- **Two build targets.** Sidecar mode adds `kaspa-pq-validator`
  to `kaspad/`. The build matrix grows by one binary.
- **Local RPC dependency.** Sidecar mode requires the node to
  expose its wRPC endpoint on `127.0.0.1` even for operators
  who would prefer no RPC at all. A future ADR may replace this
  with a Unix-domain socket; out of scope here.
- **`AllowRebroadcast` rule has to be exactly right.** A bug
  that treats a re-broadcast as `Block` makes the validator
  stop attesting after a restart; a bug that treats true
  equivocation as `AllowRebroadcast` risks slashing. The test
  matrix in PR-12.2 pins both cases; the helper is a pure
  function over two records so the unit-test surface is small.

### Neutral

- **Same-host validator + node coupling is unchanged.** ADR-0010
  already binds this; this ADR codifies the deployment shape
  inside that boundary.
- **No new consensus surface.** This is an operational ADR.
  The only on-chain artefact already exists
  (`SlashingEvidencePayload`, ADR-0009); everything new here is
  node-local (signed-epoch DB, status enum, validator binary).

## Phase 12 PR plan

Design-freeze series (this ADR + type stubs + spec):

| PR | Title | Status |
|---|---|---|
| 12.1 | This ADR | landed |
| 12.2 | `dns_finality.rs` ValidatorStatus + SignedEpochRecord + check_signed_epoch_record helper + tests | next |
| 12.3 | Spec update (ADR-0011 + Phase 12 row + v0.5) | next |

Implementation slots, all gated on the Phases 1–9 baseline being
live and on the matching ADR-0010 14-slot entries. These slots
**layer onto** the existing ADR-0010 entries (they are
implementation refinements, not new line items in the 14-slot
table):

| PR | Title | Layers onto | Status |
|---|---|---|---|
| 10.6′ | `kaspa-pq-validator` sidecar binary + 127.0.0.1 wRPC client | PR-10.6 (validator_service) | deferred |
| 10.6″ | `signed_epoch` store + `check_signed_epoch_record` integration into the validator runtime loop | PR-10.6 | deferred |
| 10.6‴ | `--dry-run` flag wiring + per-epoch eligibility log emitter | PR-10.6 | deferred |
| 10.13′ | `kaspa-pq-cli validator keygen --out` + `kaspa-pq-cli validator status` (returns the 9-variant enum) | PR-10.13 (wallet/cli) | deferred |
| 10.14′ | `getValidatorStatus` RPC + sidecar-mode smoke test on simnet | PR-10.14 (DnsConfirmation RPC + simnet smoke) | deferred |

## References

- [ADR-0009 — DNS Probabilistic Finality Overlay](0009-dns-probabilistic-finality.md)
  (defines `SlashingEvidencePayload`; this ADR codifies the
  honest-operator equivocation guard that prevents accidental
  evidence production).
- [ADR-0010 — Validator Node Architecture](0010-validator-node-architecture.md)
  (defines the in-process validator service; this ADR adds the
  sidecar shape as a recommended alternative).
- [ADR-0008 — Hash64 consensus identity](0008-hash64-consensus-identity.md)
  (Hash64 throughout the `SignedEpochRecord` fields).
- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md)
  (ML-DSA-65 signing context applies to the validator key the
  same way as to the owner key; the validator-context vs
  transaction-context separation is in ADR-0009 §"Attestation
  target").
