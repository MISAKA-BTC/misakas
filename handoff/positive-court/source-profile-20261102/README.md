# Actual CPU sample of the original short job — 2026-11-02

This diagnostic follows `b5ceb25ad` / implementation `6cd0f4dbc`, before the
scratch-reuse change. It samples the same already compiled opt2 original
Pythia short-job binary, rather than substituting a toy workload or replaying a
saved verdict. The previous goal turn made progress by implementing and
measuring hash membership; this turn uses its actual remaining cost to select
the next implementation change. The [complete goal](../COMPLETION-REQUIREMENTS.md)
remains incomplete.

The operator runs the unchanged original noncanonical 16/1 job with max context
2,048, all 213,298 required relations, the same artifact/arithmetic/inputs and
the same finite 240-s CPU / 360-s wall and memory/IO/tape limits recorded in
[the prior measurement](../optimized-runtime-20261102/README.md). Original model
and registration fixture SHA values match before and after; the binary SHA also
matches at those checkpoints ([input pins](input-sha.json)). No source file
changes during the run. Native authority, canonical/full-context execution,
isolated cold Node and physical RSS admission remain false/unqualified.

After the first logged 1,024 numeric relations, `/usr/bin/sample` attaches to
the **existing** PID 67665 for one 10-second window at a nominal 1-ms interval.
The sampler is a separate process with a 30-second wall stop. The original
runtime monitor continues; neither the target nor sampler is restarted on an
observation timeout. The [sampler record](sampler.json) reports exit zero and
no stop. [Raw CPU sample](cpu-sample.txt) and [console](sample-console.log) retain
their bytes. Profiling itself perturbs timing and adds external process costs;
this run does not qualify production latency or cold-role physical costs.

The trained worker has 7,491 observations in that window. Inclusive stack
categories include:

| Category | Worker observations |
| --- | ---: |
| Producer material construction | 4,722 |
| Producer dependency walk | 4,240 |
| Producer push/membership | 3,603 |
| Table growth/rehash | 1,030 |
| Hash-one operations | 2,943 |
| Node stream submission | 2,765 |
| Direct demanded evaluation | 2,290 |
| Source-only authentication, across both roles | 791 |
| Binding verification | 374 |
| Range analysis | 228 |

Categories overlap and **must not be added**. These are observations in one
time window, not exact CPU seconds or whole-job frequencies. The
[summary](sample-summary.json) preserves each matched frame; its script counts
only the trained worker and excludes nested double counting within a category.
The earlier [inclusive extraction](inclusive-sample-counts.json) is retained.
The sample identifies table growth as concrete remaining producer work. It
does not justify skipping source authentication, exact evaluation or coverage.

The target later exits **-24 / SIGXCPU** with no outer stop, final libtest
summary or Node verdict ([runtime record](pythia-profile-first.json)). Its last
logged numeric prefix is 40,960 / 213,298, not an exact terminal count. Both
processes are terminal and the target process group is empty. This is a failed
bounded full-job attempt with useful diagnostic evidence, not a pass.

Operator scripts and their SHA inventory are retained under `operator-scripts/`.
The raw runtime record pins source-before/after, command/environment, log SHA,
exits, sampled target-group RSS and free-disk minimum. A later normal Cargo
build reuses the same executable filename for changed source; the old SHA pins
identify the bytes actually executed here, not a promise that those bytes
remain at that generated-cache path forever.

The next change retains only empty producer walk capacity under its existing
live owner, clearing membership/pending tasks on success and every early
return. Its actual validation and measurement are recorded separately in
[trace-scratch-20261102](../trace-scratch-20261102/README.md). No native design,
funding, P approval, credit consumer or activation condition is changed.
