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
