# PALW Header v4 anti-spam: implementation and public/value re-genesis checklist

Status: **implemented as a re-genesis-only mechanism; not approved for public or valuable deployment**.

This change removes one code-level part of the free algo-4-header path. It does not close ADR-0040
gate G6: G6 remains **Measurement** until the specified header-flood benchmark establishes acceptable
per-header database-write and p99 processing bounds. Pruning-point transport, accepted block-keyed
lifecycle provenance, and operator-pinned complete-payload Header-v4 import are implemented, and the
BIND-04 / SS-01 coverage regressions are present. Permissionless Header-v4 import nevertheless remains
**StopShip** until chain-derived descendant/header-bundle authentication removes that operator trust.
Bounded retained-history reclamation, the owner-only DA spool, peer recovery/serving/GC, and lifecycle
and provider-unbond tooling are implemented. Automatic owner-key `0x3b` response submission, long
multi-node soak and calibration, and monitoring/custody/incident rehearsal remain activation blockers.

## 1. Deployment fence

All six shipped parameter presets keep `palw_spam = PalwSpamParams::INERT`. They therefore continue to
produce their existing header schema and do not activate the accumulator or stamp rule. In particular,
this change does not turn testnet-palw-110 or devnet-palw-111 into Header-v4 networks.

A non-inert configuration is accepted only when all of the following are true:

1. `palw_spam.is_structurally_valid()`;
2. PALW activates no later than genesis (`palw_activation_daa_score <= genesis.daa_score`); and
3. the new genesis declares `PALW_ANTISPAM_HEADER_VERSION` (version 4).

The node fails at construction if these facts do not hold. Header v4 is consequently not an in-place
fork switch for any current identity; it requires a distinct network identity, genesis, and data
directory.

## 2. Canonical Header-v4 fields and hashes

Header v4 appends, in this frozen order, after all Header-v3 fields:

1. `palw_spam_accumulator_commitment: Hash64`;
2. `palw_spam_nonce: u64`.

Both fields enter the block-id and pre-PoW canonical header preimage only at version 4 or later. A v3
header must carry both fields as zero, preventing serialized-header/block-id malleability.

The objective stamp uses the independent `PalwSpamHash64` domain over the complete final canonical
header. It therefore binds the final transaction merkle root, final authorization hash, fork-local
accumulator commitment, all parents and work fields, and the spam nonce itself.

The ticket authorization commitment deliberately substitutes `palw_spam_nonce = 0`, in addition to
its existing circular substitutions for the authorization hash and authorization-free transaction
root. This permits the producer to sign and finalize the authorization transaction before grinding.
A signature can be reused across nonce attempts, but this is not a free-header axis: every nonce has
a distinct complete-header stamp digest and must independently meet the objective target. Mutating the
accumulator or any other non-circular header field invalidates the authorization signature.

Header v4 also changes the meaning of `overlay_commitment_root` under a disjoint
`OverlayPalwCommitV2` domain. It folds the legacy DNS/EVM overlay root with a canonical
`PalwSelectedParentStateV2` digest covering the exact selected parent, beacon/batch/lane/nullifier
frontier, beacon accumulator, provider view, active paid-work set, DA state root, and immutable
active-batch references (`batch_id`/manifest hash, leaf root/count, exact lifecycle certificate hash).
The reference root never enumerates the mutable local present-leaf store, so a leaf arriving after a
parent cannot change that parent's root. Header-v3 uses the pinned v1 path byte-for-byte. This v2 root
is the authentication boundary for a pruning sidecar: the first post-pruning-point child reconstructs
the selected-parent state and rejects any mismatch.

Header-v4 also requires `header.palw_epoch_certificate_hash` to equal the lifecycle's exact
`cert_hash`; a different valid certificate for the same batch is rejected. Header-v3 preserves its
historical same-batch behavior byte-for-byte.

P2P carries the fields as protobuf header fields 30 and 31. gRPC carries them as fields 31 and 32.
The internal RPC binary serializer uses version 7; a version-6 stream decodes the absent suffix as
zero for legacy compatibility and cannot encode a valid Header-v4 block. Every peer, miner, and RPC
consumer on a v4 network must therefore be upgraded before genesis.

## 3. Exact fork-local admission accumulator

For candidate `B`, selected parent `P`, and configured DAA horizon `W`, the validator performs this
transition:

1. Count every newly admitted blue in `mergeset_blues(B)`, excluding `P`, by lane. This is an exact count, not
   a sampled window.
2. Add that delta to `P`'s cumulative hash/replica counters, producing the past counters used for
   `B`'s decision.
3. Set `lower = daa(B).saturating_sub(W)` and find the newest row on `P`'s selected-parent chain whose
   DAA score is at most `lower`.
4. Subtract that authenticated baseline. The result contains precisely the admission events in the
   open lower boundary of the full horizon, including all new merge blues attributed at transition
   `B`.
5. Add `B` itself to its lane's cumulative counter, increment selected height, derive the one
   deterministic skip link, and commit the complete row in `B`'s header.

Attributing a merge blue when it enters the selected history prevents an attacker from hiding many
siblings between samples. Since each row is derived only from its selected parent, competing forks do
not leak counters or shortcuts into one another.

Each persisted row commits:

- row version and DAA score;
- selected-chain height;
- cumulative hash-blue and replica-blue counts;
- selected parent, including pointer presence; and
- one deterministic bounded checkpoint/Fenwick ancestor, including pointer presence.

Let `C = next_power_of_two(W)`. Active parameters require `1 <= W <= 65,536`, so `C` is finite and at
most 65,536. Inside a checkpoint, a row clears the low bit of its selected height (the Fenwick
predecessor); a checkpoint row points exactly one checkpoint back. No skip is more than `C` selected
transitions old. Genesis is deliberately excluded from the DAA window, so a re-genesis root and its
direct children share one DAA score. One common predicate permits exactly that root-height-0 to
child-height-1 equality; every later direct edge and every multi-height skip requires a strict DAA
increase. This turns the DAA horizon into a proof that one selected-height checkpoint is sufficient:
early closures contain the root, while a row `C` transitions behind a later tip is already at or below
every relevant lower DAA boundary. Header-v3 and every inert shipped preset do not enter this rule.

Validators re-derive each skip and validate the linked row's height, boundary-aware DAA ordering, and
counter monotonicity; an attacker-supplied shortcut is not trusted. All walks fail closed after
`PALW_SPAM_MAX_LOOKUP_HOPS` (256) reads. Tests cover exact lower-inclusive boundaries, checkpoint and
power-of-two edges, fork/reorg and restart isolation, forged shortcuts, a 60,000-row / 26,440-DAA
horizon, 2,048 reproducible variable-DAA queries checked against a linear oracle, and continuation
from a pruning closure beyond the next checkpoint.

The bincode value encoding is pinned at 180 bytes. At 10 blocks/second this is 155,520,000 raw value
bytes per day (about 56.8 GB/year), before RocksDB keys, indexes, cache, compression, and write
amplification if no deletion ever ran. The former 64-hash jump-vector design would have consumed
roughly 4 KiB per block and was rejected.

After ordinary header/body pruning has committed, a separate reclaim pass enumerates and shape-checks
all accumulator rows. Every row whose header still survives and the current pruning point are closure
tips; each receives at most one checkpoint of selected-parent closure. The snapshot's support rows are
pinned retained facts, not fresh tips which would incorrectly demand a second pre-import checkpoint.
A largest-budget-first worklist coalesces overlapping closures. Only rows outside that union are
deleted in one separate RocksDB batch. Iterator, header lookup, snapshot, parent/skip closure, or
staging failure deletes zero rows (a failure after cache-backed staging is fail-stop).

For `N` stored rows, `H` surviving Header-v4 header rows, `S <= C` snapshot support rows, and `U` rows
in the distinct closure union, the pass does `N` row enumerations and header-presence checks, `O(U)`
parent/skip reads, and `O((H + U) log(H + U))` worklist operations rather than `H * C` full path walks.
The retained-row ceiling after a successful sweep is
`min(N, (H + 1) * (C + 1) + S)` before overlap; a single-chain boundary with no other retained header
tips is exactly `C + 1` rows. Side-fork/proof/anticone headers deliberately add their own bounded
closures. A fresh import starts with exactly one checkpoint, can derive descendants immediately, and
never needs a missing older checkpoint; after catch-up advances the boundary, old support is reclaimed.
At the consensus maximum, the support values themselves occupy 11,796,480 raw bytes (`65,536 * 180`)
before keys/framing; the 26,440-DAA candidate uses a 32,768-row checkpoint (5,898,240 raw value bytes).

## 4. Rate rule and objective target

For the exact counts before a prospective replica candidate:

```text
capacity            = hash_blues * replicas_per_hash + burst
prospective_replicas = replica_blues + 1
congestion_band      = min(7, floor(prospective_replicas * 8 / (capacity + 1)))
required_stamp_bits  = min(max_stamp_bits, base_stamp_bits + congestion_band)
```

All arithmetic is checked with widened intermediates. A replica is rejected if
`prospective_replicas > capacity`; overflow is an error. Active parameters require a non-zero base,
`base <= max`, and `max <= 512`.

The base objective floor is checked during isolated header validation, before GHOSTDAG, reachability,
accumulator reads, or header-stage writes. After the bounded merge-set has been computed, the full
dynamic target and accumulator transition are checked. Hash-lane v4 blocks still carry and authenticate
the accumulator, but must use spam nonce zero because their Layer-0 PoW already provides the admission
cost.

Template construction and validation call the same accumulator transition and target functions. When
an ordinary hash template is converted to a replica template, the node re-reads the latest virtual
generation and fails closed if DAA score or direct parents changed, including a side-tip-only change.
The miner then finalizes the authorization transaction and merkle root, grinds only
`palw_spam_nonce`, and returns an error on nonce exhaustion rather than emitting a partially stamped
block. Header persistence stages the cache-backed accumulator row last in both ordinary and trusted
header batches; staging or final RocksDB failure is process-fatal so a Rayon worker cannot expose a
cache-only row that disappears on restart.

`PUBLIC_REGENESIS_CANDIDATE` currently supplies:

```text
window_daa = 26,440
replicas_per_hash = 4
burst = 8
base_stamp_bits = 12
max_stamp_bits = 19
```

These values are a benchmark fixture, not a deployment recommendation or a closed gate. At 10 BPS the
horizon is roughly 44 minutes; 12 and 19 bits imply mean search sizes of approximately 4,096 and
524,288 trials respectively. G6 must calibrate them on the slowest supported producer and verifier
hardware while measuring adversarial sibling/orphan/header-only traffic.

### 4.1 G6 reproducible measurement harness

`palw_header_spam_bounded` is an ignored, measurement-only test. It clones `devnet-palw-111` into an
isolated temporary Header-v4 re-genesis fixture, opens algo-4 only inside that fixture, and drives
`ConsensusApi::validate_and_insert_block` through the ordinary production header processor. It asserts
all six shipped presets remain pre-v4, anti-spam-inert, and acceptance-closed.

Run it in release mode on an otherwise idle machine, with a new output path:

```bash
PALW_G6_REPORT_PATH=/absolute/path/to/new-g6-report.json \
cargo test -p kaspa-consensus --release palw_header_spam_bounded -- \
  --ignored --nocapture --test-threads=1
```

Defaults are 1,000 weak-stamp siblings, 1,000 weak-stamp unknown-parent orphans, 32 valid warmups, and
1,000 valid stamped header-only blocks. They can be changed with
`PALW_G6_INVALID_SIBLINGS`, `PALW_G6_INVALID_ORPHANS`, `PALW_G6_WARMUP_HEADERS`, and
`PALW_G6_VALID_HEADERS`; `PALW_G6_STAMP_MAX_NONCE` bounds fixture-side stamp grinding. The output file
uses create-new semantics and will not overwrite prior evidence.

The JSON records source commit/dirty state, OS/architecture/CPU/memory/toolchain, exact candidate
parameters, sample counts, and nearest-rank median/p95/p99 distributions. Latency starts immediately
after `Block::from_header`, before submission, and ends at `block_task` completion; block/header
construction and valid-stamp grinding are excluded. Production counters separately record successful
header `db.write(batch)` calls and the
RocksDB operations in each `WriteBatch`, plus the reachability operations and distinct reachability
data rows within that batch. Every weak-stamp sample must reject as
`PalwSpamBaseStampTooWeak` with both counters zero and no header/status/relation/accumulator row. Every
valid sample must reach `StatusHeaderOnly`, one successful timed path, and one atomic header batch.

`invocation.in_flight_headers` is exactly 1: samples are submitted and awaited one at a time. The
external latency therefore measures the serial single-task path, not queue/backpressure behavior,
worker saturation, concurrent peer ingress, dependency scheduling, or contended RocksDB. Concurrent
flood and long-soak evidence remains a separate G6 requirement.

The fixture uses a `TestConsensus`-created OS-temporary RocksDB database. It executes the production
`ConsensusApi`, ordinary header processor, and storage code, but it is not equivalent to deployed
ingress/service topology or the slowest supported production storage. Schema v2 records these limits,
along with whether the worktree is clean enough for reproducible source provenance. A dirty or
indeterminate tree makes the report diagnostic only: the commit hash does not identify its uncommitted
source, so final evidence must be rerun from a reviewed clean commit.

The first 1,000-valid-header release run on an Apple M1 Max used a dirty tree and is therefore
diagnostic, not final evidence. It nevertheless exposed a consensus-reachability blocker rather than a
threshold to freeze. After 32 warmups, total batch operations were min/median/p95/p99/max
16/547/997/1,037/1,047. Exact attribution was 2/533/983/1,023/1,033 reachability operations and
1/532/982/1,022/1,032 distinct reachability-data writes; non-reachability operations stayed at 14
(15 max). Once a parent's trailing `u64` reachability interval is exhausted, `add_tree_block` invokes
`reindex_intervals`; `propagate_interval` rewrites every existing child interval, while
`split_exponential` consumes the complete child capacity, so the next sibling reindexes again. The
per-header write bound is therefore O(nodes in the reindexed reachability subtree), not constant.
Changing that bound requires a reviewed reachability/allocation redesign or a consensus-validity
sibling bound. (Historical: this paragraph recorded the pre-remediation state.)

**2026-07-27 re-measurement with the ADR-0043 (A) bounded allocator** (`split_exponential_with_reserve`
trailing re-tile reserve + the harmonic flood-regime insertion policy in `add_tree_block`, threshold
64): the identical 1,000-valid-sibling flood on the same M1 Max now measures total batch operations
min/median/p95/p99/max **16/16/16/16/79**, reachability operations **2/2/2/2/65**, distinct
reachability-data writes **1/1/1/1/64**, non-reachability flat at 14 (15 max). Per-header cost is
CONSTANT at p99; the single max spike (79/65/64) is the ONE re-tile event at the 64-sibling threshold
crossing — the amortized-O(1) contract of ADR-0043 §Amendment, observed. The sibling flood no longer
externalizes super-linear reachability writes: the attacker's stamp spend and the network's write cost
are now a constant ratio. The G6 gate moves **Measurement → Bounded**. Threshold freeze (the stamp
ramp values and any residual limits) still requires the multi-machine serial/concurrent flood and
long-soak runs plus independent review — those remain external, and public/value activation remains
gated on them.

The report emits `gate.status = "Bounded"` and `gate.thresholds = null`. A passing harness proves the
measured path, the rejection ordering, and the bounded per-header write profile; it does not choose
acceptable limits, replace slow-hardware and multi-node flood/soak runs, or authorize a public/value
deployment.

### 4.2 DA Object V2 boundary

Header-v4 leaves are V2-only. A V2 leaf commits the object version, byte length, chunk count, root,
Receipt-v3 compute set, job challenge, issuance epoch, and expiry epoch. The canonical object carries
both provider bond outpoints, two Receipt-v3 bodies/envelopes, two bond-owner-authorized session keys,
and the exact matched-pair ID. The Qwen lifecycle exporter and node pin identical Borsh bytes, root,
length, and pair ID in `receipt_da_object_v2_golden_v1.json` on both repositories.

`ConsensusApi::palw_admit_da_object` is the only production admission seam. Under one virtual-state
read snapshot it resolves the selected-chain leaf and both provider records, then checks all of the
following before the content-addressed durable object store is reachable:

- object root/length/chunk metadata and leaf/batch/index/bond bindings;
- both owner ML-DSA-87 authorizations for the exact session key, network, bond, and validity window;
- both Receipt-v3 signatures, credentials, slots 0/1, compute/job/epoch expectations, and expiry;
- the verified two-replica match result and exact pair ID.

A caller may populate the bounded P2P serving cache only after that admission succeeds. P2P v8
`GetPalwDaChunk` / `PalwDaChunk` tracks requested roots/indices, rejects unsolicited or oversized
responses, and verifies the versioned Merkle proof. Root-only validation is deliberately insufficient;
the negative admission test includes a forged Receipt-v3 signature whose re-stamped root still matches
the leaf and proves that it is rejected before storage.

Each accepted leaf creates buried-beacon-selected, provider-specific chunk obligations. Canonical
0x3a challenge, owner-signed 0x3b response, and post-deadline 0x3c timeout evidence update fork-local
DA state. An unresolved obligation blocks certificates; live unresolved/timed-out obligations gate
rewards and exits. Valid timeout evidence stages `PalwProviderBondMutation::Slash` in the same
selected-chain transition and survives snapshot/import through the Header-v4 DA state root.

Slashing semantics are intentionally exact: the provider UTXO is not deleted and no party receives
it. `Slashed` has precedence over unbonding, reward and exit stay denied, and
`ProviderBondSpendFilter` keeps output 0 permanently unspendable. The amount and owner script remain
part of pruning-snapshot UTXO validation. This is an objective burn/forfeit, not a transferable
confiscation; operators must never treat a slashed outpoint as sweepable.

Code-level DA-01 is therefore enforced, but public launch is not complete. The default-disabled local
spool plus `misaka palw da enqueue` now moves Qwen's `export --node-context` artifact through full
admission without adding an unauthenticated network RPC. The opt-in availability service rehydrates
selected-chain retained V2 objects after restart/reorg, performs proof-verified multi-peer recovery
with deadline/backoff/failover telemetry, and atomically GC's durable rows against the complete
retained-root set rather than the smaller serving cache. See `palw-da-object-v2-operations.md`.

The service does not possess provider owner keys and cannot sign/submit an on-chain 0x3b response.
Canonical V1/V2 `da-inspect` and owner-signed `da-response` plus timeout payload/submission commands are
available for manual incident handling, but challenge discovery and deadline-aware submission are not
automated. Response automation, incident rehearsal, live withholding soak, capacity calibration, and
long-retention soak remain launch blockers. The production callers are intentionally
activation-gated: the filesystem importer and peer recovery scheduler call
`FlowContext::cache_palw_da_object`, and the scheduler constructs `PalwDaChunkRequester`, only after
the independent algo-4 acceptance lever is released. All shipped presets therefore retain zero
automatic Object-v2 upload/fetch traffic.

## 5. Re-genesis procedure

Do not edit an existing preset in place. Prepare a distinct public/value candidate as follows:

1. Allocate a new network suffix/identity, P2P identity and ports, seed policy, and empty data
   directory. Do not reuse a closed-testnet ledger or peer namespace.
2. Define a new `GenesisBlock` with `version = PALW_ANTISPAM_HEADER_VERSION`. The genesis conversion
   derives the canonical empty accumulator commitment (height and counters zero; no parent or skip).
3. Set PALW activation at genesis and configure a structurally valid, non-inert `palw_spam` value.
   Keep `palw_algo4_accept = false` until every activation gate below is satisfied.
4. Regenerate the genesis coinbase marker, merkle root, UTXO commitment as applicable, and final
   genesis hash. Extend and run `config::genesis::tests::gen_kaspa_pq_genesis_hashes`, which now uses
   the canonical v4 conversion and therefore includes the empty accumulator commitment; pin its
   output, then run `config::genesis::tests::test_genesis_hashes`. A v4 node must start from a fresh
   database; the persisted `Header` layout and accumulator store are not compatible with an older
   datadir.
5. Upgrade all P2P/RPC/miner components, create a private allowlisted rehearsal network, and verify
   hash and replica templates round-trip through P2P, gRPC, and RPC serializer v7.
6. Run G6 header-flood calibration and a long fork/reorg/IBD soak. Freeze measured thresholds and then
   replace the candidate stamp/rate magnitudes with the reviewed values.
7. Rehearse the implemented accepted block-keyed lifecycle-provenance and snapshot/import path from
   both genesis and a pruning point, and require identical acceptance
   decisions under raw-but-unaccepted manifests and invalid-attestation certificates as well as
   component tampering, provider UTXO mismatches, fork/reorg, restart recovery, and the first
   post-pruning-point Header-v4 `c == v` check. Exercise the implemented retained anti-spam-history
   sweep across one-by-one pruning points, catch-up, restart, side forks, and reorgs.
8. Exercise the implemented Qwen V2 export → local node admission → peer recovery/serving path and
   lifecycle/unbond tools in a multi-node rehearsal. Add provider owner-key 0x3b response automation,
   monitoring, incident-response, archive rotation, and key-custody procedures. Exercise DA
   challenge/response/timeout and prove that a slashed output cannot be swept. Only after
   the gate review may a separate change set `palw_algo4_accept = true` for the new identity.

## 6. Pruning and trusted sync: accepted provenance implemented; pre-install authentication remains StopShip

The complete DB-v14 snapshot/import path now captures the pruning-point accumulator row and exactly
one bounded selected-parent checkpoint (at most 65,536 support rows). Capture occurs before source-row
deletion. The exact outer Borsh object and P2P receiver share one 128-MiB cap. The Header-v3
closed-network sync path validates canonical encoding, bounds, pruning-point/header context, the
current row commitment, and the full closure, then atomically installs it with the other PALW/DA
boundary state. Certified lifecycle entries require the exact certificate plus all leaves and independently
re-derive `leaf_root`; uncertified entries canonically carry only their manifest and re-announce
partial leaves with membership proofs. `PalwSelectedParentStateV2` supplies the root a first
post-PP Header-v4 child needs to authenticate imported live state; support rows remain validation
witnesses, but canonical shape and counter monotonicity do not cryptographically bind every older
support row to the directly header-committed current anti-spam row. That `c == v` comparison currently
runs only during descendant body/UTXO validation, after staging/live installation. A digest advertised
by the same peer is not an independent authenticator, so Header-v4 peer import is explicitly refused
in both the IBD client and consensus importer until (1) live state is authenticated before the durable
write and (2) support rows are recursively committed or verified against transported headers.

Periodic pruning commits the pruning-point pointer, complete PALW/DA boundary, and required DNS
overlay snapshot in one RocksDB batch. Startup recovery may rebuild missing sidecars only while their
source rows remain and batches all repairs together; after those rows are pruned it fails closed.

This is not yet a complete public-network bootstrap. Raw mergeset observation remains available for
side-fork-deterministic body processing, but accepted manifest/leaf/certificate bytes and the exact
accepted lifecycle row are now block-keyed and staged atomically with the virtual UTXO commit. Strict
capture projects that accepted row. The raw-view seam still rejects raw-but-unaccepted manifests and
invalid-attestation certificates; the closure tests
`palw_pruning_snapshot_uses_accepted_block_keyed_lifecycle_provenance` and
`palw_pruned_ibd_matches_from_genesis_under_raw_overlay_adversary` prove that raw arrival does not alter
the transported accepted snapshot or fresh-node resolution.

The accumulator store now has the closure-aware, fail-closed post-prune sweep described in section 3.
Unit/property and real temporary-DB restart tests prove one-checkpoint import, power-of-two boundaries,
one-by-one frontier movement, side-fork retention, bounded row counts, and next-child equivalence with a
from-genesis store. Public activation still requires a long multi-node pruning/catch-up/reorg soak and
measured RocksDB/runtime limits. This is a measurement/operations gate, not a claim that permissionless
snapshot import or public activation is closed.

## 7. Honest gate status after this implementation

| Area | Status | What remains |
|---|---|---|
| Header-v4 canonical stamp, fork-local full-horizon state, template/validator parity, and transport fields | Implemented and unit/property tested | Independent review and network-level integration/soak |
| ADR-0040 G6 algo-4 header anti-spam | **Measurement / public-value StopShip** | Valid sibling measurement exposes O(reindexed-subtree) reachability row rewrites. Select and review a bounded reachability/allocation or consensus-validity design, then repeat multi-machine serial/concurrent flood and long-soak calibration before freezing thresholds |
| Pruning-point transport/authentication | **Partial / permissionless StopShip** | Operator-pinned complete-payload Header-v4 import is implemented; remove operator trust with chain-derived descendant/header-bundle authentication, then independently review and soak |
| Bounded retained accumulator state | Implemented / activation soak required | Multi-node pruning/catch-up/restart/fork/reorg soak and storage/runtime calibration |
| DA Object V2 semantic admission, local spool, peer recovery/serving, challenge/timeout, and state snapshot | Implemented and unit/golden tested | Provider owner-key 0x3b submission automation, independent review, multi-node withholding/reorg/archive-rotation and retention soak |
| Withholding resistance / PCPB | **Open / measurement and activation blocker** | Cross-device and multi-node tests, capacity calibration, operational response, PCPB completion |
| Public lifecycle and unbond operations | Tooling implemented / activation rehearsal open | Public transaction rehearsal, monitoring, rollback, incident and custody procedures |
| Public/value network activation | **Open** | All preceding blockers plus the existing ADR gate review; no shipped preset enables v4 or algo-4 acceptance |

The closed shared-testnet runbook remains the correct scope for existing PALW presets. Header v4 is a
foundation for a separate re-genesis candidate, not evidence that a public valuable network is ready.
