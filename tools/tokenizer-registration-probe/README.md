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
