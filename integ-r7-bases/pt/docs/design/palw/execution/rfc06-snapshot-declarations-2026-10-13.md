# RFC06: authentic declarations for the native snapshot probe — 2026-10-13

`scripts/drills/rfc06_snapshot_declarations_dump.rs` recovers the authentic genesis
admission bytes of the native four-population fixture through the public
`palw_t12_launch_params_v1()` assembly. The fixture's harness changes bond keys,
payouts and its premine/genesis identity; the complete-population setup adds funded
bonds and Panel parameters. Neither edits `ClassRegistered` objects. This tool
therefore uses those objects directly, and verifies their bindings against the
actual raw carriage before creating a private, create-new declaration output file.
It installs no native state and exercises no P2P server.

The public assembly contains exactly three classes: the binary BASE0 floor with
`admission: None`, and dense held rows at 8,192 and 2,097,152, each with an authentic
4,735-byte admission carriage. Their Borsh `Vec<(Hash64, Vec<u8>)>` occupies 9,610
bytes. The state-plus-declarations snapshot envelope adds 9,620 bytes to the raw
state (ten framing bytes plus that vector). The floor exception is explicit: the
tool derives its profile/canonical identity through the binary's BASE0 API and
requires the pinned genesis artifact root. It recomputes no floor weight artifact.

The tool refuses unknown/missing/duplicate classes or any mismatch in artifact
root, declared PWU, slash value, genesis registration provenance, profile/canonical
class id or fused-attention binding. It independently derives canonical PWU using
the fixture bundle's authentic **held court ladder**, rather than the legacy 2^22
cap. Every held declaration must be covered. Existing immutable semantic rows,
when present, must match the authentic canonical context; historical absence stays
absence. Its controls preserve the authentic admission bytes exactly. Output flags
separate complete declaration coverage from an unchecked whole native state root
and unchecked actual server index.

Usage after the native test passes and finishes its raw carriage output:

```text
snapshot-declarations-dump native-carriage.borsh NEW-declarations.borsh
snapshot-wire-probe native-carriage.borsh NEW-declarations.borsh
```

Compile these independent tools against the **matching producer binary's** core,
hashes and Borsh libraries. An older core may decode another carriage schema;
therefore the earlier inspection/control executable must not be relabelled as the
fresh native binary's measurement. The new dump script was absent from
`/tmp/misaka-rfc67812-runtime/final-test-v2-build-inputs.json` when that build was
pinned; it is an independent tool outside that graph. No production source changed
for this tool. The existing wire probe checks state equality and declaration
counts; full declaration byte equality must also be checked before declaring a
full snapshot codec pass.

## Actual serving gap and validation limits

The production API `palw_class_carriages_for_sync_v1` reads the class-carriage DB
index (prefix 226) for the current state's class ids. The former genesis path folded
class registrations into state without saving their admission bytes in that index;
its writers were later accepted registrations and explicit adoption. Thus a manual
complete vector derived here can measure required bytes, but cannot prove that a
fresh/pruned node's actual getter serves them. The separate RFC08 atomic-genesis
index patch and fresh-node/pruned acceptance tests must close that consumer gap.
This tool's success is not that closure.

Direct compilation against the earlier native opt3 core library passed; all four
binding controls passed, no ignored tests (0.04 s test body; 14,958,592 bytes whole
process maximum RSS). Controls construct class binding objects, not accepted native
state. They cover authentic exact bytes, missing/unknown/duplicate classes, altered
roots/PWU/canonical fields, a false admission-None exception, retained semantic
identity mismatch and fused binding mismatch. [Raw control results](../../../evidence/palw/design-records/rfc06-snapshot-declarations-2026-10-13-tests.log),
[public assembly inspection](../../../evidence/palw/design-records/rfc06-snapshot-declarations-2026-10-13-inspect.log)
and [source/library/binary hashes](../../../evidence/palw/design-records/rfc06-snapshot-declarations-2026-10-13.json)
are retained. The actual native100k raw dump, matching full-snapshot wire result,
whole-node resource bounds and multi-process P2P/pruning remain unmeasured.

The new [complete snapshot probe](rfc06-snapshot-full-wire-2026-10-13.md) separately
checks received declaration **byte equality** and full state equality, with strict
complete-fixture binding checks. It leaves the earlier state-only probe and its
pinned build inputs unchanged. Its matching-v2 controls are separate from these
earlier declaration-only control hashes.
