# Separate finite extended-cost diagnostic — 2026-11-02

Implementations: diagnostic window/counters `2abe4ff31`; geometry-derived
cumulative copy allowance `a29de5a96`; measured element allowance `db0f42fce`.
The prior goal turn made progress with bounded
empty-scratch reuse, regressions and an actual 49,152-relation CPU-limited
prefix. This increment provides a separate opt-in diagnostic window to measure
the **same entire** original trained short job past that boundary. It does not
change any production setting, native authority or activation condition.
The [full seven-stage goal](../COMPLETION-REQUIREMENTS.md) remains required.

**Current result:** r3 completes all 213,298 original short-job numeric
relations and the final EOF/root/signature check, returning local `Matched`.
Its actual ignored test passes with exit zero after 976.730 s. This is complete
local checking of the original noncanonical 16/1 job in one shared process,
not native acceptance, isolated cold-role qualification or activation. See
[the complete result](complete-short-job-r3.json) and the measurements below.

## Explicit diagnostic bounds

Both private test entry points request all 213,298 original relations. The old
entry point keeps its 240-second CPU / 4-GiB actual-read / 2^22-call window.
The new ignored `pythia70m_original_short_job_extended_cost_measurement` requires
both existing model opt-in and the literal additional environment opt-in
`MISAKA_TRAINED_COST_WINDOW=1440s-8gib-16mi-calls`. It uses CPU RLIMIT 1,440 s,
8 GiB actual requested file-read bytes and 2^24 read calls. Its operator monitor
has a separate 1,800-s wall stop. Neither window can select an unlimited value.
Run exactly one ignored case in its own process; resource limits are process
wide, not libtest-thread-local.

[Window derivation](window-derivation.json) retains the previous actual
49,152 / 213,298 prefix, roughly 1.17-GB reads and 2.31-million calls. Scaling
that prefix uniformly gives planning estimates near 1,041 CPU seconds,
5.08-GB reads and 10.04-million calls. The extended diagnostic leaves finite
headroom over those estimates. Later relations may differ; these estimates
are **not proven full-job bounds**, production capacity, fees or activation
evidence. This experiment must observe full completion or the next real
refusal instead of declaring feasibility from extrapolation.

Unchanged actual/requested bounds include the 8-GiB shared logical owner,
1-GiB evaluation workspace, 256-MiB authentication owner, per-relation
262,144 terms / 262,144 source reads, 320-MiB cumulative tapes,
2^20 opening attempts and two provider-build attempts. Failed work is not
refunded. The 4-GiB target-group RSS and 512-MiB free-disk stops remain sampled
monitors, not kernel/atomic admission quotas. Per-call provider read/scratch/
return envelopes and every source/material/record authentication rule remain.
The standard element limit remains 16,384. The initial extended runs also use
16,384; the later measured extended allowance is 32,768, as detailed below.

## Measurement and validation

The same single Node evaluation now contributes checked cumulative completed
elements, reduction terms and logical source reads. Failed relation diagnostics
distinguish prior completed work from the failed relation. Successful whole
measurement requires work observations for every relation; unknown work is
not silently recorded as zero. If all numerical checks finish, the same actual
`finish()` performs EOF/root/signature verification once, with its own elapsed
time and claim-tape read counters. No observation is a native receipt or a
substitute for authentication, coverage, final output agreement or P approval.

[Copy-corrected runtime](runtime-r3.json) passes **36 tests**: 11 full-relation,
four record-authentication and 21 wide Panel tests. All three trained opt-in
cases are ignored in this default run. The earlier window-only source also
passed [runtime-first.json](runtime-first.json) before the cumulative/final
observation additions; it is not relabelled as execution of the later source.
[Window/counter runtime](runtime-r2.json) also passed before the copy-allowance
correction. [Copy-corrected strict Clippy](clippy-r2.json) and the earlier
[window/counter Clippy](clippy-first.json) pass with `--no-deps -D warnings`.
These checks do not establish trained-model success or workspace-wide lint
cleanliness. The private opt2/debug0 overrides retain assertions and overflow
checks; no RUSTFLAGS or Rust stack override is introduced.

The trained case keeps original Pythia-70m revision
`a39f36b100fe8a5377810d56c3f4789b9c53ac42`, unchanged private wide-Q24 artifact,
class/artifact/tokenizer/context, arithmetic, input IDs and output commitment.
It is the original **noncanonical 16/1** job with registered max context 2,048,
not canonical 255/2 or full-context execution. Ordinary Miner does not build a
witness; Panel captures once. Producer and Node still coexist and retain
producer owners. SHA reads warm OS caches. This is not an isolated cold-role
CPU/RAM/IO qualification.

All eight original and current workspace tips remain ancestors of this branch
([current ancestry](workspace-ancestry.json)). The main checkout and other
ongoing work are untouched. The native-assignment RFC/ADR drafts remain
unadopted. Full local trained checking, even if it completes, cannot supply
actual native Assignment/Result/source/funding/P, credit readers, real node
recovery/dependencies or explicit finite activation.

## First actual extended run: exact copy-budget refusal

[The first actual execution](pythia-first.json) exits **101**, with one failed
test and no outer stop after 328.283 s. It completes exactly **65,529 numeric
relations** before material 65,529 is refused. This exact count is read from
the live sequential checker at the refusal; it is stronger than the last
periodic progress log, which is 64,512. No final Node verdict or complete
record EOF/signature acceptance is produced. Input/binary SHA checks still
match after exit ([input pins](input-sha.json)); its process group is empty.
The failed run is retained as a failure, not relabelled as feasibility.

[Counter diagnosis](copy-refusal-diagnosis.json) identifies `Charge::Copy`,
which independently uses the configured `max_io_bytes` ceiling. Before the
refused charge, its spent counter is **2,199,000,019,924** against the old
**2,199,023,255,552-byte** ceiling. Every material pays the same conservative
`2 * 16 MiB = 33,554,432` charge before construction. The next charge exceeds
that ceiling. This is not an arithmetic mismatch, provider read limit or
physical RAM allocation.

The spent value equals `65,529 * 33,554,432 + 211,614,752 + 30,644`:
all prior material attempts, the complete i128 claim lanes, and two original
binding exports. [Original fixture metadata](copy-refusal-metadata-check.json)
independently confirms the 15,322-byte per-binding charge. Actual provider reads
are only 1,519,009,937 bytes in 2,982,990 calls, with 50,730 openings and one
build attempt. Symbolic copy/IO envelopes are **not actual file traffic or
native service fees**. Existing admission charges remain spent on failure.

## Full-plan copy quote for the next diagnostic

The extended window now derives the original expanded plan before capture,
under the still-live bootstrap owner. It checks that this relation count also
matches the later independent signed preparation. The original model, context,
source material, arithmetic, target tiling, charge formulas and every check
remain; only this private diagnostic's cumulative copy/IO allowance is derived
from all required relations instead of the provider-only envelope.

For the actual 213,298 relations the full material-copy allowance is
**7,157,093,236,736 bytes**. Adding the existing 2-TiB headroom for prior setup,
claim values and IO gives **9,356,116,492,288 symbolic bytes**. Checked arithmetic
derives the bound; no unlimited value or counter reset is used. The standard
window keeps its prior 2-TiB ceiling. The extended actual provider cap remains
8 GiB / 2^24 calls, and CPU, RAM, source/evaluation, attempts, tape and outer
monitor limits remain as above. This is still a bounded cost experiment,
not approval of a production profile.

The next actual execution uses a fresh, explicitly monitored process only
after the first process is confirmed terminal and the corrected source passes
runtime and strict Clippy checks. Its result must be assessed separately;
the prior failure is not a continuation checkpoint, and local experiment
repetition grants no native attempt, funding or credit rights.

[Running snapshot](running-snapshot.json) records the corrected run's actual
PID/start time, current compiled SHA, source agreement, observed prefix and
full-plan copy quote. It is a historical liveness observation, **not a terminal
result**. Revalidate the exact process/session before continuing; an observation
timeout must not restart it. [Validation checkpoint](validation-checkpoint.json)
binds current runtime/Clippy checks and the failed first run, while the second
run's terminal outcome remains unproven. Its live log is excluded from the
completed raw-log digest inventory until termination.

Documentation checks retain all 43 known missing references with zero new
issues; the baseline is unchanged. [Staged whitespace audit](staged-whitespace-check.json)
passes for non-raw files and records four raw tool-log terminal-blank-line
findings. Those raw bytes and their digests are preserved, not normalized.

## Second execution: exact late-position element refusal

[The corrected-copy execution](pythia-r2.json) also fails: exit **101**, one
failed test, no outer stop, 511.433 s elapsed and an empty terminal process
group. It completes exactly **106,198 numeric relations** before the next
single direct evaluation returns `Budget`. It does not produce a full Node
verdict or whole-record EOF/signature acceptance. The original model/fixture
and that execution's compiled SHA still match after termination
([input pins](input-sha-r2.json)).

[Actual diagnostic](element-refusal-diagnosis.json): position 9, occurrence 1,
block 1 / layer 0 / node 163, original I64 shape `[8, 1, 1]`, eight target lanes.
The evaluator attempts **16,385 elements against 16,384**, with 5,280 terms,
10,339 source reads and 807,985,600 workspace request bytes. The source material
fits its authentication bound. This is the actual element `WorkLimit`, not an
arithmetic mismatch or a reason to attribute Miner calculation guilt.

Before the failed evaluation, completed work is 91,589,767 elements,
1,608,257,540 terms and 1,669,315,565 logical source reads. Producer material
preparation takes 286.8616 s and successful Node submissions 219.1559 s;
the failed submission takes another 0.0093 s. Provider usage is 2,297,640,770
bytes, 4,518,282 reads and 78,917 openings, one attempt. These are actual prefix
costs and distinct accounting units; they do not establish a full-job bound.

## Measured element allowance within the existing owner

Only `ExtendedCost` now selects 32,768 elements per relation. The standard
window keeps 16,384; every other source/evaluation, work, actual-read, tape,
attempt, CPU and memory-owner limit remains. Each relation still passes its
actual conservative workspace quote before evaluation. The local record's
policy words authenticate the selected limits, and each experiment constructs
and signs its own record. No old outcome is reused or converted to native credit.

[Workspace derivation](element-workspace-derivation.json) uses the actual pinned
compiled 560-byte memo-entry envelope and the unchanged workspace formula.
The element-limit increment adds 59,506,688 logical request bytes. Even allowing
the full 65,536-target source cap, the conservative request is 914,148,224 bytes,
within the existing **1,073,741,824-byte owner**. The context-count cap is already
bounded by the fixed original job/program, so that term is unchanged. This is
logical request accounting, not measured RSS or physical admission. Actual
quotes remain mandatory, and the whole job may still encounter another limit.

[Current runtime](runtime-r4.json) again passes 36 selected tests with three
trained cases ignored; [current strict Clippy](clippy-r3.json) passes. The
[current validation checkpoint](validation-element-checkpoint.json) binds
those checks to the current source, while preserving r2's distinct failed source
snapshot. [The new running snapshot](running-snapshot-r3.json) records the exact
new PID/start time/session, compiled SHA, declared limits and observed progress.
It is historical liveness evidence; r3's terminal result is still unproven at
that checkpoint. The r3 live log is excluded from its completed-log inventory.
The r2 failure, earlier snapshots and raw digests remain preserved.

The [later live observation](crossed-element-boundary-r3-live.json) crosses
the exact former 106,198-relation failure. At the shared 103 periodic prefixes
through 105,472, completed elements/terms/source reads, evaluation counts,
provider use and evaluator peaks all match r2, with unchanged logical live/peak
requests ([partial alignment](prefix-alignment-r3-live.json)). The workspace
quote increases by exactly the derived 59,506,688 bytes. This partial comparison
does not establish complete trained material-byte equality or a final outcome;
the new process remains subject to every finite bound and final authentication.

## Third execution: complete original short job

[The actual r3 runtime](pythia-r3.json) exits **zero**, with **one pass**, no
outer stop and an empty terminal process group. It checks all **213,298**
relations, reports one source evaluation per relation, reads all 213,298 claim
vectors and completes the existing exact EOF/root/signature authentication.
Only then does it emit local `Matched`. Its total outer elapsed time is
**976.730 s**; libtest reports 976.58 s. The earlier failed r1/r2 executions
remain failed in their own records and are not counted as passes.

| Actual phase | Elapsed wall time |
| --- | ---: |
| Original owned weights load | 0.3214 s |
| Ordinary Miner inference, zero witness runs | 0.0786 s |
| Provider's original-data index construction | 0.5192 s |
| One-pass Panel numerical capture | 0.4859 s |
| Complete claim tape / signed Panel record construction | 3.5075 s |
| All producer material preparation | **546.7153 s** |
| All Node numerical submissions | **424.0629 s** |
| Final record EOF/root/signature authentication | 0.00691 s |

The Panel's cost includes the long material-construction phase as well as its
short numerical capture. The combined verification loop is 971.658 s; the
whole process uses 957.342 s user and 18.866 s system CPU at its completion
observation. These are actual shared-process phase costs, not isolated role
CPU or universal model-speed claims. SHA checks before execution warm OS caches.

The completed evaluation observes **184,572,795 elements**, **3,036,075,176
reduction terms** and **3,156,969,214 logical source reads**. These evaluator
units are not hardware instructions, network bytes or native fees. Peak
per-relation observations are 26,282 elements, 164,864 terms, 170,720 source
reads and **867,668,864** workspace request bytes, inside the enforced
32,768 / 262,144 / 262,144 caps and existing 1-GiB owner. The logical live/peak
request ledger remains **8,053,697,455 bytes** under its same 8-GiB cap.

Actual requested original-provider reads total **4,447,335,950 bytes** in
**8,492,230 calls**, with 149,358 opening attempts. That exceeds the former
4-GiB / 2^22-call diagnostic window and stays inside the selected 8-GiB /
2^24-call window. Claim-tape requested read bytes are 212,467,949 in 426,598
calls, including the EOF probe; the canonical tape payload is 212,467,948 bytes.
Aggregate capture/claim tapes remain **305,797,608 bytes**. These counters do
not measure physical disk traffic or a native network carrier.

Sampled target-process-group peak RSS is **174,063,616 bytes** and minimum free
disk is 1,720,713,216 bytes. The same 4-GiB sampled RSS stop, 512-MiB sampled disk
stop, 320-MiB cumulative tapes and 1,440-s CPU / 1,800-s outer wall limits hold.
Sampling and logical leases do not constitute a physical/kernel admission
guarantee. The caller asserts that all introduced role guards drop and the
ledger returns to zero before test exit.

[Original model/fixture and compiled binary SHA checks](input-sha-r3.json) match
before and after execution. The same original class, arithmetic, input IDs,
token 273 and committed output are retained; Panel executes once and constructs
the same required relation count/target geometry. The
[final validation](validation-final-r3.json) checks current source agreement
with runtime-r4, strict Clippy-r3 and actual model-r3. The 36 selected regressions
and the separate actual trained pass retain their distinct scopes/counts.
The old running snapshots and partial alignment records remain historical;
they are superseded for terminal status by this actual completed runtime.

## Full objective remains incomplete

This result is for the **original noncanonical 16-prompt / one-decode** job,
not canonical 255/2 or registered full-context 2,048 execution. Producer, data
provider and Node share a process and retain producer owners. Physical cold/
warm role isolation, network/wire/storage recovery costs, maxima/tails and
global/concurrent/Fraud-protected native capacity remain unqualified.

Actual native independent Assignment/Result authority, roles, atomic funding/
resource writer, source-duty service, P-selected certification, every credit
consumer, accepted real-node histories/restart/reorg/IBD/pruning/dependencies
and explicitly approved finite activation remain required. Native-assignment
RFC/ADR drafts remain unadopted. Full local computation checking does not
replace validator approval under the explicit honest-fraction assumption.
No production setting, wire identity or native authority was changed.
