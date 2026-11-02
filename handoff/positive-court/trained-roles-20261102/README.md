# Actual trained role measurement — partial checkpoint, 2026-11-02

The seven-stage objective still requires actual LLM Miner/Panel/Node costs;
synthetic regressions and ordinary→dense self-consistency do not replace them.
This increment adds an opt-in, bounded root-workspace measurement target for
the retained original Pythia-70m artifact and its saved 16-prompt/1-decode job.
Original class, artifact, tokenizer, arithmetic, maximum context 2048 and job
context are independent fixed pins. No new native authority is introduced.

## Executable path

The caller uses the trace-free bounded metadata/parameter owner, actual ordinary
Miner inference, one production R5 Panel capture, a real signed Panel record,
and all relations derived by the actual Node checker. The test-only provider
reads bounded original-file pieces with `pread`, constructs only the ordinary
artifact identity-hash tree, and reuses existing inventory openings/multiproofs.
It never exports private resident parameters, converts the whole model to i128,
or reconstructs a full parameter instance for each opening.

The persistent provider budget charges construction attempts, all read calls,
requested bytes and opening attempts, including failed roots, guard denial and
short/failed reads. Input file stamps and the existing Merkle authentication
protect data identity. They do not prove model computation correctness.

A shared 8 GiB logical request ledger covers bootstrap/export owners, metadata,
resident weights, provider, capture, runner, builders and Node acquisitions.
The existing one-use Work retains cumulative record/tape/provider counters. A
shared 320 MiB disk reservation and RLIMIT_FSIZE supplement that Work's total
Tape charge; CPU RLIMIT is 240 seconds. An outer process-group monitor enforces
finite wall time, a sampled 4 GiB RSS stop and a 512 MiB free-disk floor. Sampled
RSS termination is not kernel memory admission. Source/model/native archives
and original raw evidence are preserved.

## Qualification boundaries

- This is the **original saved short noncanonical job**, not the native canonical
  pair and not execution at every position of its maximum context.
- Producer and Node currently coexist in one process. The Node API receives
  authenticated materials, not a parameter store, and invokes no model executor;
  this is not yet a process-isolated cold-Node RAM/CPU qualification.
- Node-loop timing separates producer material preparation from numerical
  `submit`, while whole-process CPU and maximum RSS remain cumulative shared
  observations. Input SHA reads and preceding phases warm OS/range caches.
- Native Assignment/Result/funding/P, all credit consumers, multiple-node
  recovery, final cost profiles and public activation remain uncompleted.
  The separate native RFC/ADR adoption question is still pending.

## Verified partial checkpoint

Code commit: `45a24e9fb`. Actual execution uses opt0/debug0, overflow checks and
debug assertions enabled, one Rayon thread, default libtest stack and no global
Rustflags. The operator bounds are not native policy or performance activation.

- [Default runtime](default-checkpoint.json): **16 passed**, **2 ignored**.
  Ignored opt-in model/quote cases are not included in that passing count.
- [Strict Clippy](checkpoint-clippy.json): selected executor/Panel target
  passed `--no-deps -D warnings`.
- [Geometry-only storage case](storage-checkpoint.json): one passed, explicitly
  no numerical model run and no completed Node verification.
- [Actual model run](pythia-original-short-checkpoint.json): **whole case failed**
  with `wide Panel builder simultaneous owners cap`. The independently parsed
  [phase record](phase-checkpoint.json) retains the completed prefix only.

Representative phase timings from that last actual model run:

| Phase | Wall time | Observation |
| --- | ---: | --- |
| Trace-free metadata | 0.006 s | Original pinned class/job/max-context |
| Original owned weights | 5.117 s | 103,378,910 payload bytes read |
| Ordinary Miner inference | 1.958 s | Token 273, zero witness runner calls |
| Original-file identity index | 0.616 s | 205,485 bounded piece reads |
| One-pass Panel capture | 2.391 s | One numerical pass, 14,157 leaves, 93,329,660 tape bytes |
| Panel builder / Node | Refused before record/Node | No Node numerical result exists |

The completed Panel capture still retains **2,349,455,644 logical run-request
bytes**, although its executor/scratch have finished. Producer standing requests
are 5,726,964,322 bytes at that point; whole-process maximum RSS observed there
is 164,347,904 bytes on macOS. Logical reservations are not measured physical
RAM. The next implementation task is a required working-to-retained-output
handoff, preserving coverage of all surviving data and legacy API behavior.

The original 64 MiB tape limit was reached at 67,105,285 bytes and refused the
first capture. Geometry of the unchanged class/job requires exactly 93,329,660
raw capture bytes and at least 211,614,752 i128 claim payload bytes. The existing
Node plan has 213,298 relations / 13,225,922 target lanes, giving 212,467,948
canonical Borsh claim bytes and 305,797,608 aggregate capture+claim tape bytes.
These counts do not include full source/material transport cost. A proposed
384 MiB local tape setting was not executed because the operator disk admission
check refused available space; 320 MiB covers the measured geometry with the
existing free-disk floor. No native caps or wire meanings were adjusted.

## Original failures and resource preservation

The first strict check exposed a test-only u64/u32 leaf-offset mismatch and an
unneeded mutable File binding; original diagnostics are retained. An additional
storage-quote helper's native history-width conversion failed before execution
and was corrected using checked conversion. These failed builds are not passes.
The first actual model attempt used a 64-entry tiny-fixture tensor-table cap;
the [bounded header census](original-header-census.json) observes 329 original
entries. The local parser cap became 512 without changing the file or model.
The first ordinary run refused a 1 GiB local ceiling; the existing ordinary
quote requires 2,344,624,394 bytes, and the prior trained caller's finite 4.5 GiB
setting was used under the unchanged global 8 GiB request limit. Original logs,
source snapshots and failed outcomes remain separately recorded.

Disk prelaunch refusals ran no model. Exact own previous binaries were gzip
preserved with full decompressed stream/hash verification before their cache
aliases were released. Sixteen exact previous own A4 object files were tar/gzip
preserved and each extracted stream verified before release. Empty archive-match
attempt records show zero releases and are not evidence of reclaimed space.
No source, model, raw evidence or native archive was removed, and no Rust library
was manually restored into Cargo output. All SHA reads warm OS caches.

The actual whole model flow, isolated Node costs, native roles/funding/P,
all consumers, recovery and activation remain uncompleted. See
[the full completion ledger](../COMPLETION-REQUIREMENTS.md).
