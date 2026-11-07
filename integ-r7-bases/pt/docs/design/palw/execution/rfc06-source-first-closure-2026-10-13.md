# RFC06 source-first implementation checkpoint (2026-10-13)

The user changed the execution order to finish implementation before consolidated
validation. All changes described here are **source-only and unvalidated**. No new
compile, unit test, benchmark, native run or codec probe was executed for them.
Previously recorded native fixture success and the 180-second codec timeout are
historical evidence for their pinned binaries, not evidence for this patch.

## Implemented source changes

* Protocol107 compression aggregates Borsh fields into fixed 64KiB input scratch,
  preflights the borrowed immutable carriage once, and verifies the actual emitted
  canonical count. It does not insert a zlib sync-flush. Tests source compares old
  field-write/new buffered compressed bytes, old stream decoding, sink errors,
  count overflow and CPU lease expiry. Results remain pending.
* Serialization/preflight and compressed native decoding check the original
  600-second lease periodically. Expired CPU work cannot obtain a new lifetime by
  beginning chunk continuations. No deadline extension is an acceptance result.
* P2P senders collect declarations against the captured pruning snapshot, not the
  current virtual tip. A shared immutable Arc getter avoids a full native carriage
  clone per serving session; legacy owned APIs and state codecs remain available.
* Declaration readers/adoption bind profile/canonical IDs, the retained exact
  DerivedV1 PWU enumeration, fused geometry and retained semantic identities.
  Historic MaxPerAttempt is explicitly not a canonical size proof.
* Negotiated107 complete material rejects missing/unknown/repeated declarations.
  Retained semantic identity is checked; without it, only a byte-identical
  configured genesis Some admission or the exact compiled genesis None graph is
  eligible. Unknown historical canonical identity is refused with a separate
  diagnostic. Protocol106/legacy manual fallback remains qualified as incomplete.
* IBD invokes the new atomic state/declaration API. Root owns the durable
  state/declaration/index/marker batch implementation and crash-barrier tests.
  Frozen class material can be retained for future sync; execution is still frozen.
* Explicit local `--palw-resource-admission-v2` rejects an unavailable model
  profile, missing actual holding terms, saturated/overflowing demand or unknown
  host capacity at readiness, dry-run and atomic reservation gates. Off preserves
  the old fixed-scratch fallback. Synthetic whole-capture fallback cannot turn an
  unknown model into a known bound under the explicit capability.
* BASE0 now derives producer/full/segment role demand from its actual class and
  job, with its named BASE0-KV-i8 representation, row Vec capacity/header terms,
  dense segment tiles and checkpoint/capture terms. This is its own integer cache,
  not an A16 i8 quantization. A16 explicitly refuses the BASE0 representation.
* Dense/fold/hash capture derivations include native Vec growth/minimum capacity,
  simultaneous root folds and retained/serialized material. Output logits, ids,
  native headers and the job network-id bytes are included. These are conservative
  structural terms in the existing local resource estimate; they are not measured
  RSS, calibrated fees, security review or physical fleet approval.
* Explicit `--palw-native-resource-admission-v1` keeps RAM and DB capacity separate
  from wire/canonical caps. Operator budgets are stored independently of Params,
  consensus roots and presets. Unknown trusted whole-state resource bounds refuse
  before P2P materialization/receive and at the durable import boundary. No 100k
  RSS multiplier or encoded512MiB substitution is accepted.

## Remaining implementation and acceptance requirements

**The whole native RAM/DB structural bound provider is still unavailable.** Its API
returns None. Thus enabling native resource admission currently refuses an unknown
snapshot, including the native fixture. This refusal is an implemented safety
condition, not completion of all-state resource design. A useful known bound needs
all carriage collection lengths/capacities, embedded panel engine allocations,
restoration reverse indexes, root-fold scratch, snapshot/cache/batch coexistence,
serialized durable rows, WAL and compaction copies covered by a trusted point-bound
inventory. Unsupported types/versions remain unknown rather than silently zero.

Other acceptance remains outstanding: byte-identical old/new107 compression;
full authentic100k snapshot encode/receive/state-and-declaration equality within a
predeclared deadline and memory/disk budgets; actual fresh node/pruned multi-peer
IBD through the production class index; restart/reorg and crash atomicity; native
100k resource/cost approval; all model boundary/role demand review against actual
consumers; independent resource/price calibration. New flags or passing source
fixtures do not satisfy these acceptance conditions.

## Consolidated validation plan (not executed)

1. Compile the final coordinated graph once after all source owners report stable.
2. Run new pure binding, unknown/saturated capacity, BASE0 role, compression
   equivalence/bomb/truncate/codec/nonce/point/lease/spill/session negatives and
   atomic index crash/reopen tests against that graph.
3. Reuse a pinned authentic native fixture only if its full source/input manifest
   remains applicable; otherwise predeclare a fresh opt3 900-second setup/run.
4. Predeclare an independent full codec plan with authentic declaration bytes and
   phase progress. Require canonical state and declaration byte equality and
   actual wire/RSS/disk results; a timeout remains a refusal, with no extension.
5. Execute coordinated actual work/fork/IBD/pruning/DNS/crash drills after the
   source/unit boundary passes, and preserve source hashes, logs and limitations.
