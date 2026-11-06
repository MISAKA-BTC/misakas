# Required working-to-output ownership handoff — 2026-11-02

The original trained-model checkpoint refused after Panel capture because its
completed output still retained the entire executor/scratch reservation. This
increment adds an explicit required working-to-output handoff to the existing
wide runner and uses it from the existing one-pass Panel collector. Arithmetic,
model, original job, coverage, hashes, wire identities and native credit rules
are unchanged. Production handoff commit: `113feea06`. The full seven-stage objective remains incomplete. The final measurement caller
is committed as `0ff5fa6b3`.

## Ownership and compatibility

`run_observed_with_retained_output_v2` executes the same numerical pass and
callbacks as the existing runner. After CPU, callbacks and commitment hashes,
it drops the executor, acquires a required guard covering the surviving rows,
generated IDs, leaf hashes and typed result, then releases the working guard.
Both guards coexist through acquisition. Refusal returns no result. It adds no
global fallback, replay, retry allowance, native admission or funding authority.
All older run APIs retain their original whole-run guard behavior.

Checked quote methods expose retained-output requests and the overlapping
handoff peak. The new API and Panel collector refuse an insufficient peak
ceiling before CPU or allocation guards. The Panel Work still records one use
and cumulative spent work on failed preflight and post-CPU refusal.

Three new regressions check exact versus one-below peak admission, post-CPU
output-guard refusal with no replay/refund, and equality of every output, row,
leaf hash, commitment and node-observation count against the old required API.
Existing collector/loaded-source tests now distinguish working and retained
leases while still counting exactly one numerical Panel run.

## Selected verification

- [Runner/ordinary/metadata/Panel regressions](runtime-first.json): **50 passed**,
  two opt-in cases ignored, default libtest stack, opt0/debug0, assertions and
  overflow checks enabled. These are four suites (6/18/19/7), not trained-model
  completion. Production source has not changed since this run.
- Later caller-only record/provider/workspace corrections were checked with
  the [selected Panel target](runtime-auth-bounds-r2.json): **19 passed**, two
  ignored. These overlap the 50 and are not added as independent cases.
- [First strict Clippy](retained-output-clippy-first.json) passed the selected
  executor/Panel target with `--no-deps -D warnings`; the later caller source
  passed [strict Clippy again](retained-output-clippy-auth-bounds.json).
  Dependency-wide lint cleanliness is not asserted.

Every execution record retains its command, environment, complete source hashes
before/after, raw log digest, actual compiler profile when available, terminal
status, free-space minimum and process-group cleanup. Earlier failed runs are
preserved with their original source and are not relabelled as passes.
The first authentication-bound helper build used a single-opening object where
the existing `Single` variant requires a multiproof; the original compiler
failure is retained in `runtime-auth-bounds.log` and the r2 run is the pass.

## Actual original-model path

The unchanged [original Pythia artifact and saved short job](../trained-roles-20261102/README.md)
remain pinned by file SHA, class, artifact, tokenizer, maximum context 2048,
16 original input IDs, one decode and context hash. Output remains token 273.
This is a noncanonical short-job measurement, not a full-context/native claim.

After handoff the actual capture retains **1,308,868 logical run-request
bytes**, compared with **2,349,455,644** in the earlier checkpoint. Capture's
14,157 leaves and 93,329,660 tape bytes are unchanged. Actual standing requests
after capture fall from 5,726,964,322 to 3,378,817,546 bytes; the overlap peak
includes both required leases. Logical reservations are not physical RAM.

The complete signed Panel record is now generated: **213,298 relations**,
**212,467,948 Borsh claim bytes**, and **305,797,608 aggregate capture/claim tape
bytes**. Record creation hashes and signs captured values; it does not certify
that those values were computed correctly. The independent Node then derives
its own complete plan and recomputes the source relations.

The [initial handoff run](pythia-after-handoff-first.json) refused record
admission. The [record-owner correction](pythia-stream-owner-first.json) reached
Node startup but material's declared 16-MiB provider envelope left no room for
its other inputs. The [provider-envelope correction](pythia-provider-bounds-first.json)
reached actual Node submission and refused relation zero's oversized evaluation
workspace. The [workspace correction](pythia-workspace-bounds-first.json) checked
1,680 relations before the next `Budget` refusal. Its last periodic prefix
records 1,024 checks, 12.726 s of Node `submit` and 23.701 s of producer material
preparation, with all parent owners retained. None is a completed Node verdict.

Raising the source-read allowance also raises the required source/cache backing.
The [first read-bound run](pythia-read-bounds-first.json) correctly refused its
old 64-MiB authentication owner before material zero could enter the Node.
The final caller reserves 256 MiB for that owner and the existing global 8-GiB
ledger remains enforced. A default real-proof regression checks this exact
configuration conflict before the opt-in trained case.

At final source `0ff5fa6b3`, the [actual original-model run](pythia-auth-bounds-first.json)
still **fails**: the Node advances through 1,752 relations, then returns `Budget`
at position 0, occurrence 1, block 1, layer 0, node 262 (I64, shape [512],
first lane 0, 64 target lanes). That material's preflight succeeds: 209,631 bytes,
347 units and 180,808,128 authentication/cache request bytes. Actual provider
usage there is 142,258,487 requested bytes, 281,905 reads and 1,464 openings;
nominal R5 provider-envelope debit is a separate 1,535,115,264 bytes. No global
budget was reset and no later relations or EOF were accepted.

The complete process ends by that explicit refusal after 130.763 s, with a
sampled process-group RSS peak of 176,603,136 bytes, minimum free disk
611,508,224 bytes and no remaining process-group members. Its standing logical
request peak is 8,053,697,455 bytes under the shared 8-GiB limit. The final
periodic 1,024-relation prefix separates 13.116 s of Node `submit` and 24.521 s
of producer material preparation. These timings are partial and the full
Panel record has not passed the Node's final EOF/root/signature authentication.
There is no complete `Matched` result, native Court outcome or activation.
Further work must identify and fit the actual remaining demand closure or
optimize the unchanged complete checking path under real finite owners.

A two-second [sample of that actual process](pythia-workspace-sample-first.txt)
observes both the Panel dependency walk and Node reference evaluator. It is a
short intrusive diagnostic sample, not a comprehensive CPU profile or an
unqualified performance comparison.

## Exact local scope and finite costs

The streaming Core quote includes external owners in its whole-operation
request. The local measurement caller keeps actual loaded/provider/capture/
builder/preparation owners alive through record and checker use, reads their
real live ledger, and acquires only the introduced increment. No result escapes
that scope. The unchanged 8-GiB opaque-guard ledger bounds existing plus new
requests together. External requests are not zeroed and no Core quote semantics
or native lease/funding checks are changed.

The provider is a test-only original-file piece service. It bounds one opening
to 1,024 pieces, at most 1 MiB of actual requested file reads, 4 MiB returned
material and 1 MiB scratch. Each piece performs one bounded `pread`; short or
failed reads refuse without a retry refund. Construction/read/opening counters
persist on failure. Independently, actual requested provider-file IO is capped
at **4 GiB**, read syscalls at 2^22, opening attempts at 2^20 and construction
attempts at two. Model-loading and tape IO have their separate owners/counters.

R5 conservatively debits the entire per-call envelope, not actual returned file
bytes. The finite provider-envelope total is therefore 2^20 opening attempts
x 1 MiB = **1 TiB** of nominal envelope debit; the separate cumulative R5 IO
ceiling is 2 TiB including trace/tape work. These nominal counters are not
measured traffic and do not raise the independent 4-GiB file-read cap. All are
private measurement settings, not native policy, fees or activation limits.

The [documentation check](docs-check.log) passes no-new-regressions, with the
same 43 known missing references. Its baseline was not updated. Exact executed
operator scripts are retained under `operator-scripts/` with original paths and
hashes in `operator-scripts.json`; these scripts require the pinned local inputs
and their explicit finite process controls.

The latest caller enforces 16,384 computed elements and 131,072 reduction terms
per relation, with 262,144 source reads, a 256-MiB source/cache owner and a
real 1-GiB builder workspace owner.
A larger closure still refuses. Increasing a numeric ceiling cannot change a
refusal into arithmetic success; the Node must check every original relation.
The global request limit remains 8 GiB, CPU RLIMIT 240 seconds, cumulative tape
320 MiB, outer wall 360 seconds, sampled process-group RSS stop 4 GiB and free
disk floor 512 MiB. A sampled RSS stop is not kernel memory admission.

Producer and Node still share a process and all producer parents remain live.
The Node API receives authenticated source materials and invokes no model
executor, but these observations do not qualify a process-isolated cold Node.
Input SHA and preceding phases warm OS caches. Native authority/funding/P,
all credit consumers, actual node recovery and explicit finite activation remain
required in [the full completion ledger](../COMPLETION-REQUIREMENTS.md).

## Cache preservation

Only exact own completed generated caches were archived and full extracted
streams verified before release. The manifests record exact paths/digests and
whether any bytes were actually released. Source, original model, raw evidence
and native archives were preserved. Existing readable libraries were also
transparently filesystem-compressed after content, length and mtime checks;
no missing/foreign Rust library was injected into Cargo output. See the
preservation/compression manifests here. Disk prelaunch refusals run no model.

Raw libtest logs retain their original trailing blanks and spacing. The
[whitespace check](whitespace-check.json) reports only those raw-log findings;
non-log changes pass. Evidence bytes and their recorded digests were preserved.
