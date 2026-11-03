# Observe real exact-evaluation budget failures — 2026-11-02

The previous original Pythia job stopped at Node relation 1,752 with a generic
`Budget` error. Raising a ceiling without identifying the exhausted resource
would not establish feasibility. This increment exposes local costs from the
same exact evaluation, then uses the actual failure to choose the next finite
operator bound. The full seven-stage goal is not complete. Observation APIs and regressions:
`ea26f06e2`; the updated private caller: `9a038eea0`.

## Same evaluation, observable refusal

The explicit `evaluate_node_with_diagnostics` / `evaluate_legacy_with_diagnostics`
and resident/stream `submit_with_diagnostics` entry points observe the existing
strict-wide direct evaluator. They perform one evaluation with the same limits,
charges, material authentication, canonical closure, progress and errors as the
old APIs. They do not retry a failed relation or reset its budgets.

A fixed-size local observation reports the last stage, successful conservative
workspace quote, source reads, and the actual evaluator work on success or its
`WorkLimit` error. Other failures leave work unknown; no zero/completed cost is
invented. WorkLimit counters include the evaluator's attempted-limit charge;
they are symbolic evaluator accounting, not measured hardware instructions.
Closed/early-return attempts clear stale observations. The types have
no wire/serialization/native receipt conversion and allocate no native rights.

`Complete` describes this source evaluation and its canonical closure. It does
not mean that the Panel's target agrees, that a legacy bridge/selector passed,
that every relation was checked, or that EOF/root/signature and P completed.
A signed false Panel intermediate still receives `PanelMismatch` after a
`Complete` source observation. There is no credit conversion from diagnostics.

## Regression evidence

At the same source snapshot, [runtime-measured-terms.json](runtime-measured-terms.json)
records **35 passes**: old full-relation tests 11, record-authentication tests 4,
and wide Panel tests 20. Two opt-in cases remain ignored in that default run.
These counts are not an actual trained-model success.

Normal, false-Panel, false-Miner and dynamic/state fixtures compare old versus
observed resident submission results, complete progress and all cumulative
charges. The detached-file stream still checks every relation after the
producer run is released. The new failure regression separately reaches actual
work, source-read and workspace refusals, compares old/new `Budget` and
`EndedUnverified` behavior, and checks that a closed retry cannot add progress
or preserve stale cost data. It does not mock the arithmetic evaluator.

[Core strict Clippy](clippy-core-first.json) checks the Core library as the primary
package with `--no-deps -D warnings`. [Panel strict Clippy](clippy-panel-first.json)
checks the executor test target. The later caller-only bound adjustment also
passes [strict Panel Clippy](clippy-panel-measured-terms.json). These are selected package checks, not a claim of workspace-wide
lint cleanliness.

## Actual original-model diagnosis

[The diagnosed real run](pythia-diagnostics-first.json) uses the unchanged pinned
Pythia-70m artifact, original noncanonical 16/1 job, maximum context 2048,
arithmetic, input IDs, class/artifact/tokenizer/context and output token 273.
Ordinary Miner inference still performs no witness run; Panel capture executes
once. The signed record still has 213,298 relations and the aggregate tapes
still contain 305,797,608 bytes. The whole case **fails**, not passes.

It reaches relation 1,752 at position 0 / occurrence 1 / layer 0, block 1 node
262, I64 shape [512], first lane 0, 64 targets. The new observation identifies:

| Resource | Actual observation | Configured bound |
| --- | ---: | ---: |
| Computed elements at refusal | 3,881 | 16,384 |
| Reduction terms at refusal | **131,376** | **131,072** |
| Source reads | 135,038 | 262,144 |
| Conservative evaluation workspace | 617,184,640 bytes | 1 GiB actual owner |
| Authentication/cache request | 180,808,128 bytes | 256 MiB actual owner |

The matrix-product-only count missed upstream work. The observed failure is
therefore an actual reduction-work limit, not a missing source, arithmetic
mismatch or memory admission success. Node progress and spent costs remain
unchanged by enabling diagnostics. Whole record EOF/authentication and the
remaining relations were not accepted. No native Court verdict is asserted.

The next private caller uses **262,144 terms** per relation under the same real
1-GiB workspace owner. Original model/job/target tiling are unchanged. The shared
8-GiB logical request ledger, 4-GiB actual provider-read cap, provider call/attempt
caps, cumulative 320-MiB tape cap, CPU RLIMIT 240 s, outer wall 360 s, sampled
4-GiB process-group RSS stop and 512-MiB disk-stop threshold remain enforced.
This is a finite local operator setting, not a native consensus parameter,
service fee, capability or approved activation profile. Actual performance
remains unqualified until complete checking and separate cold-role measurement.

## Measured next finite bound: CPU limit, not complete checking

[The next real run](pythia-measured-terms-first.json) passes the former relation
1,752 boundary. Its last logged numeric prefix is **3,072 / 213,298 relations**,
with one observed source evaluation per successful submission. Prefix peaks are
4,272 computed elements, 164,864 terms, 170,720 source reads and 808,025,472
workspace request bytes, all within the existing real owners and finite caps.
The prefix takes 102.305 s of producer material preparation and 51.407 s of
Node `submit`; those are separate measurements, not full-job extrapolations.
Provider usage at that prefix is 160,889,360 requested bytes, 331,661 reads and
2,542 opening attempts. The logical request peak remains 8,053,697,455 bytes.

The child ultimately terminates with **SIGXCPU (signal 24)** under its configured
240-second CPU limit. No libtest success/failure summary, final relation count,
EOF/signature authentication, complete Node outcome or native Court decision is
produced. The logged prefix is a lower bound on completed local checks, not an
invented exact terminal count. The raw record retains the signal exit, elapsed
wall time, sampled RSS, free-disk minimum and empty process group. Input file
SHA checks still match after termination. This attempt is **not a pass**.

This unoptimized opt0/debug0 path is too expensive for the selected CPU window.
The next work is performance of the complete material/evaluation path under
finite owners; merely enlarging a CPU allowance or dropping relations would
not establish an accepted production profile. Producer and Node still coexist
in one process, so the prefix does not qualify isolated cold-role costs.

## Build failures and cache scope

The first Core rebuild was terminated at the sampled disk-stop threshold and
ran no tests ([original record](runtime-diagnostics-first.json)). Its observed
minimum free space was below that threshold; a sampled monitor is not an atomic
filesystem quota. The following rebuild completed, but a new regression's
builder limits exceeded its synthetic owner settings; the original failed
20-test run remains in [runtime-diagnostics-r2.json](runtime-diagnostics-r2.json).
That test setup was fixed and r3 passed before the later caller-only adjustment.

To regain build capacity, [the cleanup receipt](retired-cache-cleanup.json)
records two obsolete, unused before-goal Rust library backups and 17 temporary
outputs of our terminal failed compiler run. Those cache copies were retired,
not preserved at their old paths. Older preservation receipts remain historical;
this receipt supersedes their current-availability claim. Current measurement
executables, original models, sources, raw logs and native library archives were
not retired. Cargo rebuilt the library normally; no absent/foreign Rust library
was copied into its output tree.

Existing valid cache files were also transparently filesystem-compressed with
readable bytes, SHA, size and mtime preserved, under the Cargo lock. See
[initial compression](existing-cache-compression.json) and
[fresh runtime compression](fresh-runtime-compression.json). Cache management
is not evidence of model correctness or performance.

## Remaining goal

The Node and producer currently share a process and producer owners remain live;
measurements do not qualify an isolated cold Node. Input SHA and prior phases
warm OS caches. Complete exact checking of this original short job, canonical/
full-context cost, native Assignment/Result/funding/P, all credit consumers,
real node restart/reorg/IBD/pruning, dependencies and explicit finite activation
remain required by [the full completion ledger](../COMPLETION-REQUIREMENTS.md).
The unadopted native-assignment RFC/ADR drafts remain unadopted.

[Final validation](validation-summary.json) records current source/binary pins,
selected checks and the actual signal exit. Executed operator scripts are
preserved in `operator-scripts/` with their original paths and SHA inventory.
Raw logs retain their original whitespace; [the whitespace check](whitespace-check.json)
reports raw-log findings only. [Final docs check](docs-final.log) has no new
regressions and retains all 43 known missing references. No baseline was changed.
