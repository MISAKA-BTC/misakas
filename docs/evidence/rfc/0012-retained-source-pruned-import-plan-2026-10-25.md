# RFC-0012 retained source and fresh pruned import plan — 2026-10-25

## Status and boundary

The source-input retention code and connected importer compiled in the parent's coordinated build.
Run7 completed PASS, including retained source publication. Actual fresh-DB
native/store pruned import Run1 also completed PASS, including catchup, a new
block and cold reopen. P2P wire, OS crash and campaign acceptance remain unexecuted.
This document is a design and source-retention record, not pruned-IBD acceptance.

The previous long Run6 does not establish successful source readiness: actual
physical body/delta removal and live terminal-receipt checks preceded its later
missing-current-PALW-carriage failure. A retained DB alone does not close that failure.
The new source must independently satisfy exact current-point serving readiness.

The existing `rfc12_process_import_crash` test uses genesis premine UTXOs and real
process killing. `ibd_parents_first` transfers headers and full bodies from genesis.
Their DB lifecycle and parent-order mechanisms can be reused; neither establishes
pruned IBD or substitutes for a non-genesis pruning proof and sidecars.

Native39 to RFC12 settlement mapping (W/PWU, anchor, operator, retention/maturity,
void) is explicitly **未策定** by the user. This plan transfers existing state under
existing import rules and introduces no settlement mapping or policy.

## Retained source inputs

The new test-only leaf is
`consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/material_typed_source_manifest_v4.rs`.
Its actual long-writer caller is after final cold-reader/GC success, before dropping
the writer and deleting its temporary provider directory. Ordinary quick fixtures
keep their existing cleanup. The long source DB uses explicit retention.

The new `source-fixture-input-v1/` directory contains exactly four input files plus
a manifest: `reader-input`, `terminal-input`, `signed-admission`, and
`signed-replacement`. These are existing Borsh inputs, not JSON policy declarations.
The last reader input binds the actual tip, simulated time, nonce and prospective
material request. The terminal input carries tail, Release block and finalized flag.
The signed carrier inputs retain actual consent/object identity for later comparison.
Provider material, context archives and GC WALs are not copied as a substitute for
successful provider GC.

The typed JSON manifest records version, exact private factory name, required long
fixture flags, genesis and existing `consensus_params_id()`, actual tip/DAA/PALW root,
reopen clock/nonce, current pruning point and snapshot root, material root, tail and
Release, and each copied file's byte length and domain-separated keyed BLAKE2b-512.
The BLAKE2b hashes are explicitly not SHA256 executable hashes. The parent externally
binds the source artifact to its pinned binary SHA256 and source inventory; the
manifest does not sign or authenticate that external record.

Before publication the writer requires the final reader tip/time/nonce to match the
live source, a finalized terminal phase, a non-genesis current pruning point, exact
snapshot key and UTXO position, completed retention markers, publicly served current
PALW carriage, and actual Release-body/delta absence. It copies the four allowlisted
files with exclusive creation and per-file `sync_all`, writes and roundtrips the
manifest, synchronizes the staging directory, renames it to the final directory,
and synchronizes the containing DB directory. The final path is logged explicitly.
A previous final directory is not overwritten.

Failure before this final retention step can leave the expensive DB without a final
manifest. Failure during publication can leave a `.partial` directory. Neither is a
successful source-readiness publication; there is no promise of recovery from power
loss in this mechanism test. The caller compiled; actual successful retention remains pending.

## Existing production import sequence to reuse

`protocol/flows/src/ibd/flow.rs` is the existing sequence, not a proposed new policy:

1. Validate the source proof against the current chain (or the existing standalone
   recovery route where applicable), apply the proof, import pruning-point history,
   then await trusted-block insertion. The normal implementation is around2289–2344.
2. Sync headers after the proof's pruning point and validate pruning-point history
   against the remote tip (around1960–1964). The selected-chain child above the point
   must exist before PALW/overlay import: the child authenticates the state as of the
   pruning point; the pruning point's own header commits its parent's state.
3. Begin a durable import session. Fetch required sidecars before destructive UTXO
   clearing; import overlay and PALW snapshot/declarations under that session.
4. Clear pruning UTXOs for the session, append actual paged rows while accumulating
   MuHash, and import the complete UTXO set. Import EVM state where required.
5. Complete the same session only after all required components share the point and
   generation. The normal sequence is around2589–2627; UTXO streaming is around2700.
6. Request missing post-point bodies through the target's existing missing-body query,
   then await ordinary block validation and virtual-state processing.

Serving uses existing `ConsensusApi` methods: `get_pruning_point_proof`,
`pruning_point_headers`, `get_pruning_point_anticone_and_trusted_data`, `get_block`,
`get_pruning_point_utxos`, `pruning_point_palw_state_shared_v1`,
`palw_class_carriages_for_snapshot_v1`, `pruning_point_overlay_snapshot`, and
`pruning_point_evm_state`. Complete declaration coverage must use the negotiated
complete-snapshot meaning, not an empty declaration list declared complete.

The actual trusted-set assembly is
`protocol/p2p/src/convert/model/trusted.rs::TrustedDataPackage::build_trusted_subdag`.
It retains the existing DAA/GHOSTDAG set, missing-parent ORIGIN handling and stable
parent-first order, including tied round lanes. The fixture should reuse it rather
than recreate those rules. Placement in a protocol/integration test or a test-only
P2P-model dependency remains to be selected; no new dependency is added by retention.

## Smallest subsequent native/store fixture

Open the retained source only after the writer closes its RocksDB handles, using
`TestConsensus::with_db`, the same private factory and `process_genesis=false`.
Reconstruct params and require matching genesis/fingerprint before serving data.
Stop foreground source mining and recheck the exact point before and after export;
a moving or incomplete point is refusal, not permission to mix snapshots.

Create a distinct empty DB with staging-equivalent genesis handling. Use the normal
proof/header/trusted sequence above, then only public session-bound import APIs:
`begin_pruning_point_import`, overlay/PALW snapshot imports,
`clear_pruning_utxo_set_for_session`, append/import UTXO methods, EVM import and
`complete_pruning_point_import_for_session`. Do not copy native database prefixes,
use fixture stable setters, replay pruned full history or install sidecars by direct
store writes. The source must already prove the old Release body and delta are gone.

After completion, obtain post-point missing bodies from the target, serve only bodies
that actually remain on the source, and process them normally. Generate one new block
on the source and accept it normally on both nodes. Compare accepted block identity,
PALW root/receipt state and full virtual UTXOs, then close/reopen the target and repeat
state checks. This tests actual store/native pruned import and a post-prune block;
wire negotiation/chunk transport/staging-manager publication remain separate.

Meaningful negative controls include omitted required sidecars keeping completion
unstable, wrong point or stale epoch rejecting delayed UTXO/sidecar input, and a PALW
root mismatch refusing publication. Test failure must retain its actual error and
cannot be converted to absence or completed import. The complete imported source
and one block are a mechanism test, not 100k useful work, K3, OS crash during this
pruned import, hardware power loss, all-model cost, market settlement or a PASS for
any previously UNKNOWN campaign gate.

## Static verification

The new retention leaf was formatted individually and scoped `git diff --check`
passed. Runtime source retention and fresh import are pending as stated above.
No worker Cargo execution or source-DB modification was used for this investigation.

## Connected fixture, compile PASS; source and import NOT RUN

`material_typed_pruned_import_v1.rs` began as an unreferenced draft. After the
source-retention commit, it is now connected as a child of the source-manifest
module, sharing the exact private manifest and ingress schema/factory. The only
additional dependency is `kaspa-p2p-lib` as a consensus dev dependency; its normal
dependencies include consensus-core, not consensus. Cargo.lock already contains the
P2P package, but its consensus dependency edge is left for the parent's coordinated
Cargo update. The parent updated only the expected consensus-to-P2P dependency edge.
This worker did not invoke Cargo or edit the lockfile.

The coordinated three-package Core/Consensus/Node private-EVM no-run build
**passed on its first attempt in4m44s**. Build log:
`/tmp/misaka-rfc-header-state-import-wave-build-20261025.log`.
The10-identity source record is
`/tmp/misaka-rfc-header-state-import-wave-source-20261025/record.json`.
Consensus pin: `/tmp/misaka-rfc-header-state-import-consensus-20261025`, SHA256
`cc92539beb6234ea2b37cf89c240c8935ebf22d941afdd5175cd76c5867e2221`.
Build duration includes dependencies and is not a cost benchmark.

The parent used `--list --ignored` to confirm the exact registered test:
`pipeline::virtual_processor::tests::g14_kernel_route_e2e::material_typed_ingress_v4::source_manifest::pruned_import::retained_source_fresh_pruned_import_and_one_native_block_then_cold_reopen`.
Inventory log: `/tmp/misaka-rfc-header-state-import-ignored-inventory-20261025.log`.
Listing verifies registration, not execution: no importer runtime test PASS is claimed.
Run7 has begun independently, but its final source manifest is not yet established;
the importer runtime remains **NOT RUN**. The live writer's DB was not opened or
changed by this fixture work.

The test parent is explicitly ignored with a reason requiring an actual retained
source. When explicitly executed, both absolute source DB and fresh output-directory
inputs are mandatory. Missing input, an incomplete source manifest, an existing
output directory, mismatched genesis/params/factory, unavailable required state or
an incomplete native import fail the test; there is no environment-unset success
return. The ignored default is not an executed acceptance result.

Only a native RocksDB checkpoint of the source is used as the experiment's scratch
source. The target is a different empty DB populated exclusively by the normal
import APIs. `ConnBuilder` currently has no read-only-open mode: the original must
be closed, and its exclusive open can perform RocksDB recovery while making the
checkpoint. No Consensus/native writer is constructed on the retained original;
all later native queries and the new-block mutation use the scratch checkpoint.
This operational caveat is logged rather than claiming an OS read-only source.

The connected test validates all four allowlisted file digests, reuses native trusted-set
assembly, checks stale-epoch and wrong-point UTXO refusal and incomplete-completion
refusal, compares both terminal generations and full virtual UTXOs after catch-up
and one new block, then closes all target owners before native cold reopen.
The source's complete declarations and optional EVM/overlay paths must genuinely
be available under its existing active rules. Before the coordinated build, the new leaf was parsed and
formatted individually. Static review also binds the Arc-backed processor before
borrowing its retention markers, so the guard cannot outlive a temporary owner.
Compilation passed as recorded above. Runtime remains pending a successfully
retained source and explicit invocation of the ignored test.

## Post-compile test-clock correction

Static review after the first compile identified an omitted runtime fixture input:
Run7's `pruning::started` enables the existing instance-local validation clock, but
`t12_reopened_chain` restores only `ctx.simulated_time`. It does not enable that
clock on a newly opened consensus. The compile-only pin above therefore does not
validate time handling or importer execution.

The importer now explicitly sets and asserts the same existing validation clock on
the scratch source, genesis proof validator and fresh target from the persisted
reader time before proof/header processing. The source's existing `arm_clock` path
then advances its clock while building the additional block. Before target acceptance,
the target's context and validation clock advance to the source's final simulated
time, which must cover the actual block timestamp. The cold reopened target enables
that final clock before reads or any later work. No timestamp tolerance, params,
header timestamp, public clock or runtime production entry is changed.

The corrected leaf's coordinated build **passed in4m15s**. Build log:
`/tmp/misaka-rfc-counting-import-clock-wave-build-20261025.log`.
The six-identity source record is
`/tmp/misaka-rfc-counting-import-clock-wave-source-20261025/record.json`.
Corrected Consensus pin: `/tmp/misaka-rfc-counting-import-clock-consensus-20261025`,
SHA256 `58021023dc1a785bebed02e8b2b477c5ab44b27fcb293b41afdb74367a1baf1d`.
The parent again confirmed the exact ignored parent in
`/tmp/misaka-rfc-counting-import-clock-ignored-inventory-20261025.log`.
This supersedes the earlier compile-only pin for the clock/telemetry fixture;
neither inventory is an executed runtime test. Build duration is not a benchmark.
Actual retained-source import remains **NOT RUN**. Run7 is still active and its
source DB was not opened by this work.

The same correction adds test-only progress logs for proof export/validation/apply,
trusted insertion, header/body transfer, UTXO import, complete snapshot import and
cold reopen. Header/body progress reports every1000 successfully awaited entries;
UTXO progress reports actual pages and cumulative rows after each appended page.
Phase completion includes the actual processed count and point/tip identities.
These logs do not alter consensus data or markers and are not acceptance gates,
throughput measurements or evidence that any unexecuted importer succeeded.


## Run7 source completed; actual Run1 import in progress

The source producer completed the full long native pruning role **PASS/exit0 in2031.15s**, with actual cold receipt/all-generation GC and final manifest publication. Source DB is `/private/tmp/rusty-kaspa/typed-pruning-retained-54741-1792894786178740000`; source tip is `9b22d48f3e4dfead8cbcfd27a8ad0d78500d09ffe20b404765f098b66c389954bbb3e0c7e88944bbe27e3f48a2e883ab3f03185f625852fe91c8bc841347210e`. The source producer exited before any importer open. Its signed-input retention manifest SHA256 is `5deb85b1eaa3410983a1947569e1c2b9b3c7b5da94f2809094faf78339006988`; its producer binary SHA256 is `81afb1da4aed27e35de3b95762d77db77f8d9cf5f0d6f57b109da27eaf3201fe`. This manifest itself is not a signed campaign attestation.

Root invoked the explicit ignored parent from corrected importer pin `58021023dc1a785bebed02e8b2b477c5ab44b27fcb293b41afdb74367a1baf1d`, with fresh run directory `/tmp/misaka-rfc12-pruned-import-run1-20261025`. Parent handle22954; controller `/tmp/misaka-rfc12-pruned-import-run1-controller-20261025.json`; log `/tmp/misaka-rfc12-pruned-import-run1-20261025.log`. Actual proof/trusted insertion,74,920 post-point headers, session-bound snapshot/UTXO import and completion have passed their assertions. Ordinary post-point body processing is in progress (15,000/74,920 observed). No finished import/new-block/cold-reopen PASS is yet claimed. Earlier NOT RUN paragraphs describe the pre-invocation history. The source remains separate from scratch server and empty target; no DB prefix copy is used to establish target state.

## Run1 terminal native/store import PASS

The parent completed naturally with **1/1 PASS in1147.57s**, exit0, at UTC2026-10-25T03:14:00.578210. All74,920 post-point bodies passed ordinary insertion. Source and target then agreed on actual tip, PALW state root, virtual UTXO rows and both original/replacement finalized receipt bindings. A newly built source block `d1ddd23ef727d3654e8fc6301e3598e11c99f4d3d32729975c8108402a6a0d2dd05550f5974a4e45a165d5b4b43ffd293dbe7f56714d5129028f63dbf4629123` was accepted by the target as StatusUTXOValid and the same comparisons passed again. All target DB owners were closed, the actual target DB reopened cold, and sink/stable flag/virtual UTXO rows/PALW root/receipt bindings still matched. Both cold-reopen phase completion and the final PASS marker were emitted before the test's terminal success.

The controller above is now terminal PASS/exit0; importer binary/source identities and immutable source manifest remain as recorded. The original source, scratch checkpoint and fresh target are retained separately. The producer manifest's earlier NOT RUN scope is immutable historical source metadata, not a live experiment-status record. Earlier PENDING paragraphs record intermediate checkpoints and are superseded by this terminal result. This is actual native/store pruned import, not a P2P protocol transfer, staging-manager switch, process-kill recovery, K3,100k useful work or any of the15 UNKNOWN campaign gates. No acceptance policy or public activation value changed.
