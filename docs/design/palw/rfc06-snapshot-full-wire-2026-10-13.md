# RFC06: complete native-fixture snapshot codec probe — 2026-10-13

The independent `scripts/drills/rfc06_snapshot_full_wire_probe.rs` measures the
native four-population state together with its **complete authentic held declaration
bytes**. It reuses the production snapshot sender/receiver module and the strict
[declaration-binding tool](rfc06-snapshot-declarations-2026-10-13.md). It offers no
state-only mode. The declaration file must exactly equal both matching public
genesis admissions; wrong count, byte length, content or trailing data is refused.
After actual chunk transfer and decode it separately checks full state equality
and full declaration byte equality, including order; counts alone cannot pass.
The BASE0 admission-None class is explicitly classified as a binary-derived profile
with the pinned genesis root, with no weight-artifact recomputation claimed.

Usage after the native producer test terminates successfully and its raw file is
finalized, with executables linked against the same producer's library graph:

```text
snapshot-declarations-dump-v2 native-carriage.borsh NEW-declarations.borsh
snapshot-full-wire-probe-v2 native-carriage.borsh NEW-declarations.borsh
```

The full probe reports exact canonical/wire bytes, chunk/declaration counts and
encode/receive-decode times. `fullFixtureSnapshotMeasured=true` means this **codec
fixture** completed with all its required declaration material. It does not claim
that an actual P2P server supplied the declarations or adopted a pruning snapshot;
`actualServerIndexChecked=false` and `realP2pAccepted=false` remain explicit. It
also does not prove an arbitrary registered class set, actual model execution costs,
whole-node RAM bounds or approved economics.

Both tools are independent of the pinned final-test-v2 Cargo graph; the new files
were absent from that build's input manifest. Its native executable contains the
negative-cache commit tickets but **not** the later atomic genesis class-index patch.
A manual complete declaration vector cannot close that actual serving gap. Fresh
and pruned node checks of the separately rebuilt index patch remain required.

The dumper, full probe and their combined control executable compiled against
producer core `28b14660ab28fb31` and matching native-v2 hash/Borsh/Tokio/flate2
libraries. UUID uses the existing independent flow dependency and carries no native
core types. All **20 controls** passed, no ignored tests (2.41 s body;
41,189,376 bytes process maximum RSS): the four authentic binding controls plus
sixteen production snapshot accounting/CRC/bomb/truncate/cap/nonce/point/resume/
session/cleanup controls. These are fresh matching-codec controls, not the 100k
wire result. [Raw controls](evidence/rfc06-snapshot-full-wire-controls-2026-10-13-tests.log),
[compile command](evidence/rfc06-snapshot-full-wire-controls-2026-10-13-command.json)
and [source/library/producer/executable hashes](evidence/rfc06-snapshot-full-wire-controls-2026-10-13.json)
are retained. The renewed native100k run and actual full-fixture byte measurement
are still pending; no failed or incomplete native run is relabelled accepted.
