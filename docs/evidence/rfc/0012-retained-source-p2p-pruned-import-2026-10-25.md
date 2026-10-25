# RFC-0012 retained-source TCP pruning import — 2026-10-25

## Status

The native/store retained-source import Run1 passed in1147.57s, recorded separately in
`/tmp/misaka-rfc12-pruned-import-run1-20261025.log`. That execution transferred through
native APIs; it is not evidence of P2P wire transport or staging-manager publication.
The new TCP fixture compiled, but actual Run1 is **INTERRUPTED_NOT_PASS** after
a staging startup refusal. No TCP pruning import PASS is established.
Static source review did not open the retained source DB or invoke Cargo.

## Missing connection and boundary

Existing loopback tests provide real `FlowContext` / `Adaptor` socket construction.
They often use `TestConsensusFactory`, whose staging method is unimplemented, or a
fixed manager whose staging operation panics. Those adapters cannot establish a
headers-proof IBD adoption. The new fixture uses the actual production `Factory`,
`Ctl`, `ConsensusManager::new_staging_consensus` and `StagingConsensus::commit_if`.
No alternate mock `ConsensusFactory` implements the switch.

Run7's private factory preserves premine amounts/outpoints but changes one main
owner and derives its own genesis. Normal `Factory::new` rederives canonical premine
and refuses that profile, correctly. Its canonical-genesis check remains unchanged.
Only the initializer following that check is shared with a `cfg(test)`-only exact
retained-fixture constructor. That constructor checks the rederived fixture's
params fingerprint, genesis header and actual premine MuHash. It accepts only a
seeded Existing management entry. It is not compiled into a feature-built private
node or exposed through Args. Consequently this test covers the private fixture's
real native factory/staging/manager and TCP flows, not full daemon startup or the
normal canonical premine construction gate.

The original source must be closed. After manifest, four file hashes, factory and
params validation, a native RocksDB checkpoint becomes an independently owned scratch
source. Opening the original can perform RocksDB recovery because ConnBuilder has
no read-only mode; this caveat is logged. The original is never opened by Consensus
or used to build the additional block. No source prefix is copied into the target.

The target begins with the same fixture's independently initialized genesis and its
actual pinned premine only. Real management APIs reserve an entry and durably save
it as Existing for each node; max keys and directory names are not fabricated.
Subsequent staging directories and the durable active-directory switch are created
by the ordinary Factory and Ctl. The old active instance is stopped and reset
handlers run through the ordinary manager. Whole-DB checkpointing is source-only.

## Actual wire route and observations

Both endpoints use production `FlowContext` connection initialization and real TCP
`Adaptor` sockets. The source announces a tip inventory message; the target must
request and process the proof, trusted blocks, headers, authenticated snapshot and
UTXO stream through the shipping IBD flows. The test never calls direct pruning
proof application, sidecar import, UTXO import or completion APIs on the target.
There is no fallback to the earlier native/store fixture.

The negotiated dialect is derived from a read-only metadata accessor using the
same production constants as handshake and route registration. Both peers must
negotiate exactly the current build's version, covering the native complete-snapshot
threshold and current history-capable dialect. Current/complete/history thresholds
and actual negotiated values are logged. No fixture integer weakens the gate to an
older dialect. The present constants are108 /107 /108 respectively. History-page
retrieval is not exercised merely by negotiating that capability.

Production routing supplies pruning proof and anticone/trusted data, headers,
`RequestPruningPointUtxoSet`, overlay/PALW snapshots, and EVM state where active.
Native PALW snapshot complete material and declarations use the compressed/chunked
107-or-newer route. The actual source's existing overlay/EVM activation remains
unchanged. An inactive lane's normal not-found/skip response is not evidence of an
active sidecar restoration; this fixture does not arm missing lanes to broaden its
coverage.

The fixture requires all of these independent observations:

- the actual Factory created at least one staging instance;
- durable management changed the active directory;
- the actual manager invoked its reset handler;
- the target closed a nonzero native import epoch and published stable UTXOs;
- its accepted tip, PALW root, complete virtual UTXOs and native terminal receipts
  for both stored consent generations match the scratch source;
- one additional native heartbeat is accepted at the source and reaches the target
  through a new inventory/ordinary relay, with the same state checks;
- after socket/flow/manager owners release, cold Factory startup reopens the real
  management-selected active directory and preserves those comparisons.

Tests retain no old target native Arc while waiting for production inactive-directory
cleanup. A weak registry only observes actual Factory-created instances and their
management-selected path. It does not supply state, fake publication or validate
an object. The instance-local clock is restored on source/target active and every
staging instance before worker startup. The additional block uses the native slot
adapter; both clocks cover its actual stamp, and cold reopening retains that final
time. No timestamp tolerance, header timestamp or consensus parameter is relaxed.

## Invocation and remaining scope

The parent test is explicitly ignored by default. Explicit execution requires
absolute `MISAKA_RFC12_PRUNED_SOURCE_DB_V1` and a new
`MISAKA_RFC12_PRUNED_WIRE_RUN_V1` directory, plus the recorded fixture flags.
Missing source/manifest, old output, mismatched inputs, unsupported wire/state,
failed adoption or timeout fails the test. An unset source is never a no-op PASS.
The planned parent suffix is
`wire::retained_source_tcp_pruned_ibd_publishes_real_staging_and_relays_one_new_block`.

The shared native factory initializer and read-only protocol metadata accessor
are the only normal-code refactor/addition; the exact fixture, registry and clock
hooks are `cfg(test)`. Required new consensus dev dependencies are shipping P2P
flows, address manager, mining manager, P2P mining rules and tower counters. The
parent coordinates Cargo.lock refresh and all compilation/runtime execution.

No new public activation, economics/settlement mapping, model screening or model
tier is introduced. Formal native39 settlement mapping is 未策定. K3 inputs and
campaign gates remain separate. Real OS crash during this wire transfer, hardware
power loss, remote-host/multiprocess behavior, active EVM/overlay coverage not present
in this source, arbitrary older protocols and full daemon Args startup remain
unverified. None is established by the native/store Run1 or this unexecuted test.

## First compile and import correction

The first coordinated build failed (exit101), log
`/tmp/misaka-rfc-historical-wire-wave-build-20261025.log`. Seven diagnostics in the
new wire test were import/scope errors: the synchronous manager Service trait was
imported from the asynchronous task-service module; AttestationMempoolPolicy used
a private import path; and the existing miner-data/one-shot-selector/template-mode
helpers were not in scope. The missing Service trait also prevented start/stop
method resolution. The same build had a separate historical EVM constant import
error outside this fixture's owned scope; it is not attributed to the wire test.

The test now imports `kaspa_core::service::Service`, the public mining-policy
reexport, the existing parent-test miner/selector helpers and the public native
TemplateBuildMode. No constructor, guard, wire dialect, manager, state or policy
behavior was changed for these corrections. Individual parsing/formatting and
`git diff --check` passed. Retry compile is **PENDING** and actual TCP import remains
**NOT RUN**. The parent inspected Cargo.lock's five existing workspace dev-dependency
edges separately; this worker did not update the lock or execute Cargo.

## Retry compile and actual TCP Run1 refusal

The retry1 coordinated four-package build **passed in2m16s**, with all26 frozen
source hashes matching. Build log:
`/tmp/misaka-rfc-historical-wire-wave-build-retry1-20261025.log`.
Source record:
`/tmp/misaka-rfc-historical-wire-wave-source-retry1-20261025/record.json`.
Consensus pin: `/tmp/misaka-rfc-historical-wire-consensus-retry1-20261025`, SHA256
`c4ea75106a51d579a872fd025f0d97b28c2d30a11b03d6abc3a410e659058632`.
This is compile acceptance only. It does not supersede the failed first build's
history or establish a successful wire execution.

Actual TCP Run1 was explicitly invoked with the completed retained source and new
scratch output. Log: `/tmp/misaka-rfc12-pruned-wire-run1-20261025.log`.
Both real handshake directions negotiated108; the logged production thresholds were
current108, complete snapshot107 and history capability108. The target remained at
its private genesis, with zero created staging instances and no manager publication.

A background Tokio worker panicked at `ConfigBuilder::build`
(`consensus/core/src/config/mod.rs:354`), reached by the production
`Factory::new_staging_consensus` through `ConsensusManager` and `IbdFlow`.
The staging method clones the Config through
`to_builder().skip_adding_genesis().build()`. That build validates the entire V2
ruleset and refused the private fixture's armed `palw_probabilistic_constraints_v1`:
the binary's `PALW_KERNEL_ROUTE_ARMING_RECORDS_V1` list is empty. The earlier exact
fixture constructor enabled an Existing active fixture but did not authorize this
normal staging ConfigBuilder gate. This is a real startup refusal, not successful
proof/snapshot/session import or a reason to weaken ordinary activation validation.

The parent verified the test PID68562 and intentionally sent SIGTERM at
`2026-10-25T03:46:02.959743Z` after observing the unrecoverable background panic.
The test child exited-15 after92.33s. Although the controller wrapper itself exited0,
the amended controller classification is **INTERRUPTED_NOT_PASS**, preserving the
raw result. Stop record:
`/tmp/misaka-rfc12-pruned-wire-run1-controller-stop-20261025.json`.
The source, scratch checkpoint and incomplete target remain retained for diagnosis.
Elapsed time is not a throughput/cost result. Neither handshake success nor wrapper0
is promoted to a pruning-import PASS.

No Rust correction or arming override is introduced for this result. Any later
scoped private-fixture validation seam needs separate review and must keep normal
Factory/ConfigBuilder readiness refusal, the exact source params and the unapproved
campaign status intact. The completed native/store Run1 remains a different test.
