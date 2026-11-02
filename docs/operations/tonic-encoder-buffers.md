# Reproduce the tonic encoder buffer regressions

The vendored tonic 0.14.5 encoder releases oversized raw scratch and avoids retaining an empty tail of an emitted output allocation. Highly sparse compressed output is copied into a compact frame only when its backing allocation is at least four times the wire length and has more than 1 MiB spare. Large plain frames move their backing allocation without another full-message copy. The default reusable-buffer ceiling is 1 MiB; an explicitly larger codec buffer setting remains respected.

These ownership changes preserve protobuf bytes, gRPC framing, gzip level and message-size limits. A queued application payload, an in-flight frame and lower HTTP/2 buffers can still consume memory. This change does not impose a global outbound-byte budget.

## Run the nine focused tests

Install the repository's pinned Rust toolchain and cache its dependencies beforehand. The command below uses offline Cargo only. It refuses a work directory that already exists or is inside the repository:

```sh
python3 scripts/verify/tonic-encoder-buffers.py \
  --work-dir /tmp/misaka-tonic-regression-01 \
  --timeout-seconds 900 \
  --max-target-bytes 1610612736
```

The runner writes sources, a separate Cargo target, command logs and `result.json` only under that new directory. It never operates a node, installs dependencies or edits the repository or registry source. Runtime, target-allocation and log bounds stop only its own subprocess group. For a memory/CPU limit, run it under a caller-owned cgroup; the runner itself does not change host or node settings. Missing offline dependencies fail without an online fallback.

The runner copies the full vendored source and the same nine tests into an isolated test workspace. Its temporary manifest enables gzip only and uses exact versions from the repository's `Cargo.lock` for every selected direct dependency. It omits unused optional transport/TLS/zstd and unrelated upstream benchmark/property-test dependencies; this is a focused unit harness, not the full tonic default-feature suite. The generated lock must contain only registry package version/checksum rows already present in the repository lock. A second `--offline --locked` metadata check freezes that graph before compilation.

**The upstream `vendor/tonic/Cargo.lock` is not the dependency-version authority.** It is preserved as an upstream archive file but is replaced by the repository lock in the temporary workspace before Cargo runs.

Compilation uses a fresh target and `--no-run --message-format=json-render-diagnostics`. The runner checks the absolute manifest/source/package identity, requires a newly compiled root artifact, hashes that executable, verifies its exact nine test names and invokes that executable directly. This prevents confusing two same-name standalone packages sharing a Cargo target. The source and executable hashes are checked again afterward.

The cases cover unchanged gzip/plain output bytes, oversized scratch release, large-to-small reuse, sparse compressed-frame backing, held slow-consumer frames, encoder drop, partial encoder error and repeated held frames. Small-message reuse remains covered.

## Recorded focused result and limits

The 2026-11-02 comparison used the same nine cases and repository-pinned dependency graph in two fresh target namespaces. The original test revision compiled pristine tonic and reproduced six ownership/retention failures (three cases passed); the candidate compiled into a different, independently bound executable and passed all nine cases. That revision did not negatively detect retained raw bytes on the partial-error path, so the error case now also asserts that raw scratch is empty. Its strengthened baseline/candidate result must be recorded separately; prior evidence is not relabelled. Earlier incomplete harness attempts are retained separately and are not counted as successful candidate validation.

This result establishes the scoped encoder ownership behavior. It does not identify a production node's live heap owner, demonstrate that all observed memory growth is resolved, exercise full HTTP/2 transport integration, or replace the normal node build and boot checks. Decoder scratch and application queue byte accounting are separate concerns.
