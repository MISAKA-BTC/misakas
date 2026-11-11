# Tokenizer registration parser probe

Offline RFC-0051 measurement tool. It has no RPC, wallet or chain mutation.
It calls the production deterministic rendering verifier with the existing
rooted limits. It does not qualify the entire paid registration service.

Build from an identified commit with the pinned compiler:

```sh
cargo build --locked --release -p misaka-tokenizer-registration-probe -j 1
```

Generate fixtures in a new directory, separately from measurement:

```sh
target/release/misaka-tokenizer-registration-probe prepare /tmp/tokenizer-cases /path/tokenizer.json /path/table.palwttb
/usr/bin/time -v target/release/misaka-tokenizer-registration-probe run /tmp/tokenizer-cases actual 5
```

Run each case in a fresh process: `actual`, `maximum_dense`,
`last_leaf_mismatch`, `giant_string`, `sparse_legal`, `sparse_out_of_range`,
`duplicate_key_last`, `many_keys`. Each iteration emits JSON and asserts
its expected success or exact refusal. Dense input covers maximum raw bytes,
vocabulary and EOG count together; the giant string nearly fills the rendering
byte limit. The key-heavy case fills the raw limit with unique object keys.
These cases cover different resource stresses, not every legal input.

Record source commit, toolchain, build command, binary and input hashes,
machine resources, competing workloads, raw stdout, exit status, and the
external peak RSS/CPU report. Fixture generation is excluded from that RSS.
Input loading remains included; individual `elapsedNs` measures the verifier
only. First and repeated iterations are reported separately, with no hidden
warm-up. `conservativeScratchBytes` is a reservation formula, not measured RSS;
the tool does not reserve or enforce the native memory budget.

The actual native caller also checks accepted parent identity, rehashes Part
rows, verifies signatures, manages reservations, and retains cached results,
journals and snapshots. Measure those paths and physical storage separately
before adopting a public policy. Parser-only time or RSS cannot establish a
public throughput, fee, activation height, or complete T12 claim result.

## Native replay

Export the opted-in native fixture from the same source revision. The
functional exporter can use a non-optimized build. For the actual-input case,
set `TKR_MEASUREMENT_CASE=actual`, `TKR_MEASUREMENT_RAW` and
`TKR_MEASUREMENT_TABLE`. The default `maximum_raw` case uses a separate valid
8 MiB tokenizer and class.

```sh
TKR_MEASUREMENT_EXPORT=/tmp/tkr-native-max \
  cargo test -p kaspa-consensus-core --lib measure_tokenizer_registration_native_workload -- --ignored --nocapture
/usr/bin/time -v target/release/misaka-tokenizer-registration-probe native-run /tmp/tkr-native-max
```

The exporter writes the initial accepted-parent carriage and purpose-signed
TKR1 payload files to a new directory. It writes the manifest after its final
assertions. The manifest contains the test policy, the nondefault transition
inputs, the expected roots and fees for each action, and the snapshot boundary.

The runner uses production core APIs and checks the exact transaction IDs,
fees, journal sizes and state roots against the fixture. It reports separate
timings for fee rehearsal, rollback, authoritative folding, snapshot
encode/restore and journal undo/redo. Take external peak RSS and CPU readings
from a fresh process per bundle. Record the bundle file hashes as well as the
binary and source provenance.

The starting state and transaction funding are seeded measurement inputs.
This replay covers the native state transition from that starting point.
Node UTXO script validation, P2P transport and physical database storage
require their own measurements.

Native replay reports whether logical and resource admission are enabled.
For a separate run with declared logical limits, set both
`TKR_MEASUREMENT_LOGICAL_RAM_BYTES` and `TKR_MEASUREMENT_LOGICAL_DB_BYTES` to
finite positive byte counts. These limits exercise logical admission. The
runner does not install a filesystem quota.

Use `native-run-multipass BUNDLE_DIR` to run the same single-carrier
candidates through two fee projections and rollbacks, two observed
rehearsals, and one authoritative fold. Each candidate shares one Finish
cache across those passes. Both observed prefixes must have the same root,
and the authoritative root, fee, and journal size must match the bundle.
This uses the bundle's one-source policy and one Finish, with separate
timings for each pass. It does not measure a full mergeset or its physical
carrier scanner.

The book reserves against a single source partition. For equal-cost
Finishes, the per-DAA slot upper bound is the smallest of the pending
session limit and the four source-budget dimensions divided by their
corresponding Finish costs, rounded down. Other booked packets can lower
the available slots. This fixture has one control per source partition
and every Finish consumes one control, so its upper bound is one Finish
slot per DAA even with independent classes. A candidate can carry other
physical controls that the scanner charges and the native fold refuses.
That physical carrier count is separate from the book's slot bound.

`native-scan-max BUNDLE_DIR SOURCE_COUNT` scales a copy of the measurement
policy to preserve the one-control source budget across the supplied number
of modeled sources. It charges malformed duplicates and unresolved Finish
duplicates in separate runs. It verifies that a second copy exceeds one
source's budget and that a copy beyond the modeled source count exceeds the
candidate budget. The reported scanner timings include these refusal checks.
This measures the physical accounting functions only. No actual DAG blocks,
wallet validation, native folds, or parser invocations occur in this mode.
The scaled policy is a local measurement input.

### Modeled 780-carrier native candidate

Export a new bundle with `TKR_MEASUREMENT_SOURCE_BLOCKS=780` and the desired
tokenizer case. The exporter signs Open, Part, and Finish under that local
policy, which keeps the same one-control source partition as the one-source
bundle. The manifest records the policy and the expected single-Finish
transition. The bundle codec is unchanged.

Run `native-run-candidate BUNDLE_DIR`. The last action models 780 physical
copies of the same signed Finish, each with a distinct synthetic UTXO
outpoint and an increasing accepted ordinal. The source and candidate
scanners charge every copy. Each of two fee trials rolls back each body
before the next, so every copy reaches Finish authorization and Part checks.
Both observed rehearsals keep the completed prefix. A single authoritative
fold then processes all copies. The resulting state root and journal size
must match the single-Finish oracle, exactly one slot must be consumed, and
every projected fee must match the authoritative fold. The other 779 copies
are terminal no-ops after completion. The five passes share one Finish
derivation key in the production cache for this parent and accepting point.

This exercises a conservative native candidate workload with synthetic
funding. It does not construct a DAG with 780 legal source blocks, admit
780 Round bodies, or measure the public T12 profile.
