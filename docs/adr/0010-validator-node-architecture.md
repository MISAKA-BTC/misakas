# ADR-0010: Validator Node Architecture (operational supplement to ADR-0009)

Status: Accepted (Phase 11 design freeze; implementation deferred to Phase 10 PR series)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0001](0001-network-isolation.md) (kaspa-pq is a new
            network), [ADR-0002](0002-mldsa65-p2pkh.md) (ML-DSA-65),
            [ADR-0008](0008-hash64-consensus-identity.md) (Hash64
            identifiers), [ADR-0009](0009-dns-probabilistic-finality.md)
            (DNS overlay consensus rules).

## Context

[ADR-0009](0009-dns-probabilistic-finality.md) fixes the
consensus-side design of the DNS Probabilistic Finality Overlay —
attestation payloads, partial certificates, the two-dimensional
reorg gate, the three-stage rollout, the long-range bound
`U ≥ R + E`, public-claim discipline. It is silent on one essential
operational question:

> **How does a node operator actually run a validator?**

The natural — and wrong — answer is "replace `kaspad` with a
dedicated validator binary". DNS-style PoS adds confirmation, not
block production; turning a Kaspa node into "a validator instead of
a full node" loses the PoW/GHOSTDAG full-node responsibilities the
validator depends on (StakeScore can only be deterministically
recomputed from on-chain shards, which requires a full validating
node). The correct answer is the inverse: **a validator is a full
node that has additionally enabled a validator subsystem.**

This ADR codifies that operational architecture so the Phase 10
implementation PRs (10.4 — 10.9) write against a stable contract.

## Decision

### Node-role separation (binary stays single)

Three logical roles, all served by the same `kaspa-pq-node` binary
through opt-in flags:

| Role | Flag | Behaviour |
|---|---|---|
| **Full node** | (default) | PoW / GHOSTDAG / DAA / tx-and-UTXO validation / pruning. Validates everyone else's blocks; produces none. Validates StakeAttestationShard payloads and maintains the StakeScore store. |
| **Miner** | `--enable-mining` | Adds the upstream block-template builder + PoW solver. Block-template policy reserves mass for attestation shards (see §"Block template policy"). |
| **Validator** | `--enable-validator` | Adds the validator service that signs selected-chain anchors with the local ML-DSA-65 key and submits attestations. **Requires** the node to also be a full node (it is, by default). |

Invariants:

- Every validator **must** be a full node. Validator can't operate
  on a light-client view because StakeScore reconstruction needs
  full block bodies.
- Not every full node is a validator. The default behaviour is a
  pure full node (PoW validator, not PoS validator).
- Miner and validator are independent. They can run on the same
  process, on separate processes against the same node via RPC, or
  on entirely different machines.
- The validator subsystem is **in-process** with the consensus
  pipeline by default. A future ADR may add an RPC-based "remote
  signer" mode for HSM-backed deployments; that mode is out of
  scope here.

### CLI surface

```text
kaspa-pq-node \
    --network kaspa-pq-{mainnet|testnet|simnet|devnet}     # ADR-0001
    [--utxoindex]                                          # required by validator
    [--enable-mining]
    [--enable-validator
        --validator-key <path-to-ML-DSA-65-key>
        --stake-bond  <bond_outpoint_hex>
        [--validator-mode {offchain|shard|full}]
        [--max-attestations-per-block <u16>]
    ]
    [--rpclisten-borsh 127.0.0.1:27110]                    # PR-7.2 ports
    [--rpclisten-json  127.0.0.1:28110]
```

`--validator-mode`:

| Mode | Behaviour |
|---|---|
| `offchain` | Sign and **only** P2P-gossip raw attestations. Do not submit shard transactions. Useful during the Bootstrap phase of ADR-0009 §"Three-stage rollout" or for diagnostic operators. |
| `shard` | Default. Gossip raw attestations + submit own attestations as shard transactions when own validator is included in upcoming `StakeAttestationShardPayload` proposals. |
| `full` | Shard mode + opportunistically aggregate other validators' attestations from gossip into shards (useful if the operator runs a high-bandwidth node). |

CLI subcommands surfaced through `kaspa-pq-cli` (extending PR-5'):

```text
kaspa-pq-cli wallet create
kaspa-pq-cli wallet create-validator-key   --out <path>
kaspa-pq-cli stake bond                    --amount <sompi>
                                           --validator-pubkey <path>
                                           --owner-address <kaspapq:...>
kaspa-pq-cli stake unbond                  --bond <bond_outpoint>
kaspa-pq-cli stake status                  [--bond <bond_outpoint>]
kaspa-pq-cli validator status              [--node <wRPC URL>]
kaspa-pq-cli validator submit-slashing     --evidence <path>
kaspa-pq-cli get-dns-confirmation <block_hash>
```

### Subsystem file layout

The implementation lands across the existing crate structure
without creating new top-level crates. Concrete target paths
(implementation deferred to PR-10.4 — PR-10.9):

```text
consensus/core/src/dns_finality.rs              # type stubs (landed PR-10.3, extended PR-11.2)

consensus/src/processes/stake_registry.rs       # active / unbonding / slashed
consensus/src/processes/stake_score.rs          # deterministic aggregation from shards
consensus/src/processes/validator_sortition.rs  # PoC deterministic; mainnet ADR-pending
consensus/src/processes/dns_confirmation.rs     # WorkDepth × StakeDepth gate
consensus/src/processes/slashing.rs             # evidence handling + bond burn

protocol/p2p/src/messages/stake_attestation.rs  # raw attestation gossip wire format
protocol/flows/src/v8/stake_attestation.rs      # gossip flow integration

database/src/stores/stake_registry.rs           # active/unbonding/slashed bonds
database/src/stores/stake_score.rs              # per-anchor StakeScore entries
database/src/stores/stake_certificate.rs        # per-(epoch,target) aggregated shards
database/src/stores/slashing.rs                 # accepted evidence + reporter reward

rpc/core/src/model/dns.rs                       # RPC view types (DnsConfirmation, …)
rpc/core/src/api/dns.rs                         # RPC methods

wallet/core/src/staking.rs                      # stake bond / unbond tx builders
wallet/pq-cli/src/validator.rs                  # validator-key + bond CLI subcommands

kaspad/src/validator_service.rs                 # in-process validator loop
```

Naming convention is `stake_*` for everything that lives below the
consensus interface (state-machine state and consensus rules) and
`dns_*` for everything that exposes confirmation information to
external consumers (RPC types, CLI views).

### Validator service runtime

The validator service is a single async task spawned by the
`kaspad` binary when `--enable-validator` is set. Its loop:

```text
1. wait until consensus.is_synced()
2. fetch (current_epoch, active_validator_set_snapshot)
3. if local validator is not eligible this epoch -> sleep until next epoch
4. if local validator already signed this epoch -> sleep
5. pick selected-chain anchor for epoch
6. construct attestation message (BLAKE2b-256 with
   ATTESTATION_MESSAGE_DOMAIN key, see ADR-0009 §"Attestation target")
7. ML-DSA-65 sign with ATTESTATION_MLDSA65_CONTEXT
8. p2p gossip raw attestation
9. (--validator-mode shard | full) submit as
   StakeAttestationShardPayload tx to local mempool
10. update local "already_signed_epochs" guard
11. on consensus.on_virtual_chain_changed_notification -> goto 1
```

Eligibility (`is_eligible_this_epoch`) is the conjunction of:

```text
- bond is in stake_registry::active_bonds
- bond.activation_daa_score <= current_daa_score
- bond is not in stake_registry::slashed_bonds
- bond is not in stake_registry::unbonding_bonds
- validator is in the epoch validator-set snapshot
  (sortition output; PoC = stake-weighted deterministic; mainnet TBD)
```

The signing-once-per-epoch guard is local state stored at
`~/.kaspa-pq/validator-state.json`. **Double-signing is a slashable
offense** (see ADR-0009 §"`SlashingEvidencePayload`"); the local
guard exists to prevent honest operators from accidentally
double-signing across node restarts.

### On-chain vs P2P gossip split

ADR-0009 already specifies the on-chain commitment requirement.
This ADR codifies the P2P side of the same split:

| Surface | Carries | Consumed by | Consensus input? |
|---|---|---|---|
| P2P `StakeAttestation` gossip | One raw attestation | Other validators (for liveness), miners (for shard inclusion) | **No** — node-local view of attestations differs |
| `StakeAttestationShardPayload` tx | ≤16 attestations | All full nodes | **Yes** — every node aggregates identically |

A node that has not received a gossiped attestation can still
compute the canonical `StakeScore` because the shard transaction
contains the same attestation bytes. Gossip is for shard-inclusion
latency; on-chain commitment is for consensus determinism.

### Block template policy (miner-side)

```text
BlockTemplatePolicy {
    max_attestations_per_block:      u16,    // ADR-0009 fixes 8-16
    max_attestation_shard_mass:      u64,    // reservation
    reserve_mass_for_normal_txs:     u64,    // do not starve normal txs
}
```

Block-template builder algorithm:

```text
1. select normal txs from mempool up to
   (max_block_mass - max_attestation_shard_mass).
2. select StakeAttestationShardPayload txs up to
   max_attestation_shard_mass.
3. drop any shard whose total ML-DSA-65 verify cost exceeds
   the reserved sig-op budget (mass_per_sig_op = 6000 from
   Phase 6 reinforcement, ADR-0005).
4. emit block template with combined payload set.
```

This guarantees that a high-attestation epoch cannot starve normal
user transactions of block space.

### Header vs body validation split

ADR-0009's mainnet two-dimensional dominance rule cannot run at
header-only validation time because `StakeScore` depends on
on-chain shard payloads (block bodies). The pipeline splits as
follows:

| Stage | Rules |
|---|---|
| Header validation | PoW, DAA, pruning, parent validity, `blue_work` accumulation. **No DNS gate.** Pure PoW behaviour, identical to upstream Kaspa. |
| Body / acceptance validation | `StakeAttestationShardPayload` content checks, signature batch verification, `StakeScore` aggregation, `DnsConfirmation` update, **then** the two-dimensional dominance gate on candidate reorgs. |

The PoC hard-checkpoint mode can run at either stage and is a
testing convenience, not a claimed property (ADR-0009
§"Public-claim discipline").

### Operator runbook (8 steps)

The reference flow for an operator who wants to run their node as
a validator. Documented inline so the Phase 10 implementation PRs
can be tested against it end-to-end on simnet/devnet:

```bash
# Step 1: clone + build
git clone <kaspa-pq-repo>          kaspa-pq
cd kaspa-pq
cargo build --release --bin kaspa-pq-node --bin kaspa-pq-cli

# Step 2: full node sync (no validator yet)
./target/release/kaspa-pq-node \
    --network kaspa-pq-testnet \
    --utxoindex \
    --rpclisten-borsh 127.0.0.1:27210

# Step 3: validator key
./target/release/kaspa-pq-cli \
    validator keygen --out ~/.kaspa-pq/validator.mldsa
# emits: validator_pubkey (1952 B), validator_pubkey_hash64,
#        validator_id (Hash64)

# Step 4: bond stake
./target/release/kaspa-pq-cli \
    stake bond \
        --amount 100_000_000_000 \
        --validator-key ~/.kaspa-pq/validator.mldsa.pub \
        --owner-wallet ~/.kaspa-pq/wallet.kpq
# emits: bond_outpoint, activation_daa_score

# Step 5: wait for activation
./target/release/kaspa-pq-cli stake status --bond <bond_outpoint>
# PendingBond -> ActiveValidator at activation_daa_score

# Step 6: restart in validator mode
./target/release/kaspa-pq-node \
    --network kaspa-pq-testnet \
    --utxoindex \
    --enable-validator \
    --validator-key ~/.kaspa-pq/validator.mldsa \
    --stake-bond <bond_outpoint> \
    --rpclisten-borsh 127.0.0.1:27210

# Step 7: verify validator service
./target/release/kaspa-pq-cli validator status
# synced / bond_status / current_epoch / eligible_this_epoch
# last_signed_epoch / attestations_gossiped / attestations_included
# missed_epochs / slashable

# Step 8: spot-check DNS confirmation on a block
./target/release/kaspa-pq-cli get-dns-confirmation <block_hash>
# pow_confirmed / dns_confirmed / work_depth / stake_depth
# expected_dns_confirmation_seconds / risk_bound
```

Steps 1, 2, 5, 8 are useful even for non-validator full-node
operators; steps 3, 4, 6, 7 are the validator-specific surface.

### Phase 10 PR plan (refined per this ADR)

ADR-0009 §"Phase 10 implementation order" listed the original
PR-10.4 — PR-10.9 deferred slots. With ADR-0010 in place, the
refined slot plan is:

| PR | Title | Status |
|---|---|---|
| 10.1 | ADR-0009 (DNS overlay) | landed |
| 10.2 | Spec update (DNS scope + Phase 10) | landed |
| 10.3 | `consensus/core/src/dns_finality.rs` type stubs | landed |
| 11.1 | This ADR (validator node architecture) | landed |
| 11.2 | `dns_finality.rs` Hash64 identifiers + registry / snapshot / state types + helpers | next |
| 11.3 | Spec update (ADR-0010 + Phase 11 row) | next |
| 10.4 | Stake transaction kinds (subnetwork_id route) + tx validation | deferred |
| 10.5 | `stake_registry` / `stake_score` consensus processes + stores | deferred |
| 10.6 | `validator_service` in-process loop + `--enable-validator` flag | deferred |
| 10.7 | PoC hard-checkpoint reorg gate (`--dns-mode hard-checkpoint`) | deferred |
| 10.8 | Mainnet two-dimensional dominance rule + property tests | deferred |
| 10.9 | Validator sortition (PoC deterministic; mainnet commit-reveal in a follow-up ADR) | deferred |
| 10.10 | P2P `StakeAttestation` gossip + flow integration | deferred |
| 10.11 | Miner block-template policy reservation for shards | deferred |
| 10.12 | `slashing.rs` evidence pipeline + bond burn + reporter reward | deferred |
| 10.13 | `wallet/staking.rs` + `kaspa-pq-cli` stake/validator commands | deferred |
| 10.14 | `DnsConfirmation` RPC type + wRPC/WASM bindings + 8-step runbook smoke test on simnet | deferred |

Every deferred slot is gated on Phases 1–9 baseline being live and
on the preceding slots in this list having landed. Slots are not
forced into a single committer — each is small enough to be a
self-contained PR.

## Consequences

### Positive

- **One binary, three roles.** Operators do not pick a separate
  validator binary; they add a flag. Misconfiguration ("ran the
  validator but forgot to sync") becomes impossible because the
  validator service refuses to start until the node is synced.
- **Validator failure cannot fork the chain.** Block production
  stays PoW. A validator that crashes / misses signs / equivocates
  does not stop new blocks from being produced — it only stops
  contributing to `StakeScore` (and risks slashing on
  equivocation).
- **Operator runbook is testable end-to-end.** The 8 steps above
  are the simnet acceptance test for the Phase 10 implementation
  PRs: a CI run that exercises clone → keygen → bond → activation
  → validator restart → DnsConfirmation lookup is the gate on
  PR-10.14 landing.
- **Subsystem boundaries are explicit.** The file-layout table
  gives every Phase 10 sub-PR a target directory; reviewers can
  see at a glance where new files belong.

### Negative

- **Key management on a hot node.** The validator service reads
  the ML-DSA-65 signing key from disk at startup. Operators who
  want HSM-backed signing need a follow-up ADR introducing an
  RPC-based remote-signer mode; until then, the validator key
  lives next to the node.
- **`--enable-validator` requires `--utxoindex`.** UTXO indexing
  is needed to look up stake-bond outpoints on demand. Operators
  who do not run a UTXO index today must enable it before
  becoming validators.
- **Local "already-signed-this-epoch" guard is non-consensus.**
  The guard prevents accidental double-signing across restarts,
  but a maliciously deleted guard file does not protect against
  slashing — the slashing rule is consensus and will burn the
  bond if `SlashingEvidencePayload` is submitted. This is by
  design: in-consensus slashing is the safety property.

### Neutral

- **Existing Kaspa mainnet nodes are not validators.** ADR-0001
  established that kaspa-pq is a new network; this ADR clarifies
  the operational implication: an existing `kaspad` cannot be
  upgraded into a kaspa-pq validator by config alone. Operators
  must run the `kaspa-pq-node` binary against the kaspa-pq
  network and bond fresh stake.

## Validator-set commitment derivation (consensus-input)

`validator_set_commitment` appears inside every attestation
message and must therefore be byte-deterministic across nodes.
The derivation:

```text
sorted_validators_e =
    active_validators_at_epoch(e)
        .sorted_by(|a, b| a.validator_id.cmp(&b.validator_id))

snapshot_bytes_e =
    epoch.to_le_bytes() ||
    (sorted_validators_e.len() as u32).to_le_bytes() ||
    for each v in sorted_validators_e:
        v.validator_id.as_bytes() ||                      # 64 B
        v.stake_amount.to_le_bytes() ||                   # 8 B
        v.activation_daa_score.to_le_bytes()              # 8 B

validator_set_commitment =
    BLAKE2b-512(
        key   = b"kaspa-pq-validator-set-v1",
        input = snapshot_bytes_e,
    )
```

The key string `b"kaspa-pq-validator-set-v1"` is consensus-fixed
and bumped only by a hard-fork ADR.

## References

- [ADR-0001 — Network isolation](0001-network-isolation.md).
- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md)
  (signing scheme; `ATTESTATION_MLDSA65_CONTEXT` is distinct from
  the tx context, ADR-0009 §"Attestation target").
- [ADR-0005 — Mass / DoS policy](0005-mass-policy.md)
  (`mass_per_sig_op = 6000`; reservation arithmetic for the
  block-template policy).
- [ADR-0008 — Hash64 consensus identity](0008-hash64-consensus-identity.md)
  (validator_id, target_hash, validator_set_commitment all
  `Hash64`).
- [ADR-0009 — DNS Probabilistic Finality Overlay](0009-dns-probabilistic-finality.md)
  (consensus rules this ADR supplements with operational
  architecture).
