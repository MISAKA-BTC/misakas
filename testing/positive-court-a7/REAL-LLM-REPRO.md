# A7 retained trained-LLM local repro harness

This harness uses the three existing trained artifacts inventoried at Root
`4753ee4beae5e245095954c6ef6e9b96fa05f361`. The original contexts remain
8192 / 8192 / 2048. The saved natural-text calibration sequence zero supplies
16 prompt IDs per model; the initial planned job decodes one token. The local
job executes `prefill + decode - 1` positions. This short job does not establish
maximum-context coverage.

The Rust entry point is [src/bin/real_llm_repro.rs](src/bin/real_llm_repro.rs).
The process driver is [real_llm_process.py](real_llm_process.py). Input provenance
is [real-llm-inputs/manifest.json](real-llm-inputs/manifest.json). The previous
[inventory and proposal](evidence/real-llm-inventory-4753-2026-11-01/README.md)
remain unchanged.

## Execution boundary

At this stage Cargo, builds, model inference and runtime quotes have not been
run. Root is using the shared build window; A7 is restricted to source, review
and evidence preparation. An existing target/cache or unrelated binary must
not be selected automatically. Build and runtime results will require their
own fixed candidate, commands, binary hashes and original logs.

The driver never builds, downloads, transforms a model or creates a build
cache. Its default mode only inventories inputs and constructs commands in a
new output directory. No binary is required to record that preflight; the
manifest explicitly records the missing binary as a blocker to execution.
An output directory is never reused.

```sh
python3 testing/positive-court-a7/real_llm_process.py \
  --out testing/positive-court-a7/runs/real-llm-preflight-NEW \
  --mode dry-run
```

For future `quote-only` or `run` modes the driver requires an explicitly
supplied binary, its expected SHA-256, a fixed source commit and all six
resource ceilings plus an explicit `--external-retained-bytes` allowance.
The runner requires explicit `--quote-only` or `--run`.
Start with quote-only for all three artifacts before admitting inference;
save the full quote, independent live-owner accounting and the acceptance
decision. No successful cap values or measured costs are supplied by this
source-only stage.

The quoted backing limits start after artifact startup. Even quote-only opens
the artifact, decodes nested container headers, compiles the program, builds
the parameter instance map and performs weight identity setup. Allocation
before limits in those startup stages is **not established** by this harness.
The fixed SHA/pinned cohort and a successful runtime quote do not establish
a bounded startup reader. Core/loader hardening is a separate dependency.

## Meaning of a result

The fixed artifacts are mandatory; a failure does not select another model,
an early artifact, a toy, or a lower context declaration. SHA-256 and inventory
roots establish data identity. They do not establish computation correctness
or model quality. Model tier, review and promotion are outside this harness.

Pythia uses the explicit i64 Q24 CPU runner and `PALWTRC2` capture path,
preserving natural-log units multiplied by 2^24. The two Smol models retain
their narrow path. Commitment wire lanes are 8 bytes only for the final wide
logits; internal executor storage and independent owners are accounted for
separately.

The result is `SELF_CONSISTENCY_ONLY`: capture roots taken from the same
producer run permit a local roundtrip, not an independent expected-root
comparison. Independent expected execution, native V2 admission,
SecurityCertified/credit and full context coverage remain NOT_RUN. Empty-leaf
trace-only authentication is distinct from a dense capture; a full result
requires every expected leaf in canonical order with its membership checked.
Data authentication is separate from a Court arithmetic verdict.

## Measurements

CPU run/capture quotes describe logical backing and symbolic work. They are
separate from fresh-process `/usr/bin/time -l` measurements. On Darwin, peak
RSS is recorded in bytes and block I/O counters in operations. A fresh process
does not imply a cold filesystem cache. The driver records each model's raw
stdout, stderr, time output, exit/timeout and process-group cleanup. A zero
exit alone is insufficient for success; the runner's declared result and
fixed input identity must also match.

For a runner failure, `runner_diagnostic` retains its reported error, phase,
status, CPU-start flag and logical quote bounds beside the requested cap.
`RESOURCE_LIMIT` remains a rejected result; the driver does not increase caps
or rerun automatically. A quoted excess is not measured RSS. A generic failed
status without the explicit error is not silently classified as a resource
limit, and an error field cannot be hidden behind a success status.

The driver separately records its outer process boundary: wall timeout covers
the runner process group from launch, including startup. It does not cover
the Python parent's source/input hashing. The Darwin RSS figure covers the
runner's whole lifetime, including startup; it cannot isolate startup RSS.
RSS is observed after cleanup and **no hard RSS limit is installed** by this
driver. OOM/kill/timeout or missing resource output remains a failed/incomplete
attempt, never a startup-memory admission. Dry-run records these policies
with observation status NOT_RUN.

Work ceilings apply to the named CPU/geometry/authentication operations, not
to a certified total instruction or cumulative fee bound for the whole
harness. The legacy narrow API exposes a conservative kernel-work estimate;
it does not supply the wide runner's full host-work quote. Identity setup,
serialization, summaries and independent owner allowances are recorded
separately. Standalone logical admission does not install a native host lease.

`RUST_MIN_STACK` is an environment setting for Rust-created threads. It does
not set the OS main-thread stack. Historical Pythia's initial stack value was
unrecorded, and its successful retry explicitly used 64MiB in a test runner;
that history does not certify this standalone executable's stack needs.

Binary and source hashes are separate pins until a preserved build record
connects them. Read-only source review, Python preflight and formatting do
not replace compilation, Rust tests, actual inference, or native integration.

The focused driver guards are reproducible without a model subprocess:

```sh
python3 -B testing/positive-court-a7/test_real_llm_process.py \
  --driver testing/positive-court-a7/real_llm_process.py
```

Their synthetic resource-parser fixture is not an RSS measurement. Their
mock cancellation check is not an inference run.
