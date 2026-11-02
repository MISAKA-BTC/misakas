# Separate finite extended-cost diagnostic — 2026-11-02

Implementations: diagnostic window/counters `2abe4ff31`; geometry-derived
cumulative copy allowance `a29de5a96`. The prior goal turn made progress with bounded
empty-scratch reuse, regressions and an actual 49,152-relation CPU-limited
prefix. This increment provides a separate opt-in diagnostic window to measure
the **same entire** original trained short job past that boundary. It does not
change any production setting, native authority or activation condition.
The [full seven-stage goal](../COMPLETION-REQUIREMENTS.md) remains required.

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
1-GiB evaluation workspace, 256-MiB authentication owner, per-relation 16,384
elements / 262,144 terms / 262,144 source reads, 320-MiB cumulative tapes,
2^20 opening attempts and two provider-build attempts. Failed work is not
refunded. The 4-GiB target-group RSS and 512-MiB free-disk stops remain sampled
monitors, not kernel/atomic admission quotas. Per-call provider read/scratch/
return envelopes and every source/material/record authentication rule remain.

## Measurement and validation

The same single Node evaluation now contributes checked cumulative completed
elements, reduction terms and logical source reads. Failed relation diagnostics
distinguish prior completed work from the failed relation. Successful whole
measurement requires work observations for every relation; unknown work is
not silently recorded as zero. If all numerical checks finish, the same actual
`finish()` performs EOF/root/signature verification once, with its own elapsed
time and claim-tape read counters. No observation is a native receipt or a
substitute for authentication, coverage, final output agreement or P approval.

[Current selected runtime](runtime-r3.json) passes **36 tests**: 11 full-relation,
four record-authentication and 21 wide Panel tests. All three trained opt-in
cases are ignored in this default run. The earlier window-only source also
passed [runtime-first.json](runtime-first.json) before the cumulative/final
observation additions; it is not relabelled as execution of the later source.
[Window/counter runtime](runtime-r2.json) also passed before the copy-allowance
correction. [Current selected strict Clippy](clippy-r2.json) and the earlier
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
