# Compute-registry record pre-delivery for catch-up IBD (ADR-MA §21.4, protocol v104)

**Status: shipped 2026-08-01.** Fixes the testnet-20 fresh-sync fail-stop every participant hit
after the first algo-4 blocks were mined (`registry-active PALW header … unknown compute_set_id …
(no registered descriptor)` at the same header from every peer).

## The bug

On a registry-active net, two header-stage consumers resolve the content-addressed Compute Set
registry records a v5 algo-4 header commits:

* `palw_per_set_sublane` (`consensus/src/pipeline/header_processor/pre_pow_validation.rs`) — the
  §13.2 resolution feeding the §12 per-set difficulty check;
* `palw_source_compute_credit` (`consensus/src/processes/ghostdag/protocol.rs`) — the §14
  compute-work credit an algo-4 blue source contributes to fork choice.

But the records are only written by (a) the §21.2 fold over ACCEPTED transactions — i.e. after
body replay — or (b) a pruning-point snapshot import (headers-proof IBD only). Catch-up IBD
validates ALL headers before replaying any body, and a young net whose pruning point is still
genesis never takes the snapshot path — so a fresh sync validated algo-4 headers against an empty
registry and fail-stopped, deterministically, at the first one. Nodes that had processed bodies
in live order (the already-synced population) held the records and kept running: the network
stayed healthy while every JOINER was locked out.

## The fix — deliver the records before the headers, as §21.4 always required

Protocol v104 adds a request/response pair (protobuf oneof tags 79/80):
`RequestPalwComputeRegistryRecords` → `PalwComputeRegistryRecords`, carrying a Borsh
`PalwComputeRegistryPruningSnapshotV1` with a ZERO pruning point and an EMPTY view — a
records-only package (descriptors / policies / plans / activation certificates).

* **Serve** (`protocol/flows/src/v8/request_palw_registry_records.rs` →
  `palw_compute_registry_records_package`): the complete content-addressed record dump,
  canonicalized. The tiers are fork-independent append-only pools, so the full dump covers every
  record any served header can commit.
* **Request** (`IbdFlow::sync_palw_compute_registry_records`): runs on ALL THREE IBD paths before
  any header download — `Sync` and `PruningCatchUp` stage into the active consensus,
  `DownloadHeadersProof` into the STAGING consensus (post-boundary headers can commit records
  registered after the pruning point, which the boundary snapshot does not carry).
* **Import** (`import_palw_compute_registry_records_package`): validates canonical form, then
  stages each record under its RECOMPUTED content id (write-once; identical re-delivery is a
  no-op). The fork-local VIEW is never imported on this path — views stay §21.2 fold-derived.

### Why peer-supplied records are safe

Records are self-authenticating (content-addressed). A peer can at most pre-supply preimages it
knows — exactly what body replay would admit later. Governance truth is NOT the record pool: the
§23.4 virtual-stage seam (`verify_header_references` against the fork-local view) still decides
what GOVERNS on each fork, and a header whose committed ids were never actually registered
on-chain still dies there. The †caveat: header-stage §13.2's "unknown id ⇒ forged" inference is
only as strong as the local pool, and an IBD peer can seed the pool; the consequences are bounded
by the §23.4 seam plus `COMPUTE_TO_HASH_CAP`, and confined to the chain that peer serves.

### panic → reject

`palw_per_set_sublane`'s missing-policy/plan arms were `panic!` ("the trusted-data package must
deliver…"). With a peer-facing delivery path, a withheld record would have been a REMOTE CRASH
TRIGGER; both arms now return `RuleError::PalwComputeSetResolution` (header rejection), so a
misserved IBD attempt aborts cleanly and the next peer is tried. The GHOSTDAG credit panics stay:
they are unreachable for admitted sources (a source's own pre-pow already required its records,
and the pool is append-only).

## Compatibility

* **Consensus semantics: unchanged.** No params changed — `consensus_identity_hash` stays
  `1c48963f…`; no re-genesis, no identity re-pin. d44e7ac-and-earlier binaries remain
  consensus-compatible peers.
* **Protocol: 103 peers still negotiate** (explicit back-compat arm). A ≥104 syncer WARNS and
  syncs without pre-delivery from a 103 server (failing closed at algo-4 headers exactly as
  before); a 103 syncer against a ≥104 server behaves exactly as today. Fresh-syncing a net with
  algo-4 history therefore requires a ≥104 peer — upgrade the anchors first.
* **Known residual (pre-existing, unwidened):** a relay race where an algo-4 header arrives
  before the virtual fold of the chain block that registered its records still rejects (policy /
  plan tiers formerly crashed). The window is registration-to-mint, hours in practice; miners
  must not mint against a set whose governance settled seconds ago.

## Operational note (testnet-21)

t21 had ZERO algo-4 blocks when this shipped, so nothing needed replaying. **Do not enable PALW
(algo-4) mining on a registry-active net whose anchors do not yet run ≥104** — the moment an
algo-4 block exists, every pre-104 fresh sync fail-stops on it.
