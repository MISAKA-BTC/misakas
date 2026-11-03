# Remaining source capacity — 2026-11-02

The common Court/source replay previously checked consumed usage alone. A
Court charge could fit that check while consuming capacity still needed by an
accepted source request. The documented gap is now guarded at the local replay
boundary, without assigning a new native resource projection or authority.

## Implementation

- `positive-contract::reservations::RemainingCapacity::check` checks consumed
  wire/work/retention plus outstanding demand with checked arithmetic, while
  retaining all existing signed service-budget checks. It neither charges
  usage nor creates a physical reservation. No default reservation is supplied.
- `NativeCommonUsagePolicyV2::remaining_capacity` is required for every common
  replay event. The historical owner receives the actual updated source ledger
  and proposed Case, including its new accepted origin, and must resolve all
  protected remaining demand and service opportunities.
- The check precedes publication of the new Case/cursor. Failure poisons the
  entire local replay cache and exposes no partial receipt, due or state. It
  creates neither a Miner default nor a calculation conviction. Native policy,
  physical leases, interval/deadline opportunity and the atomic native writer
  remain required and unprovided.

The two new integration regressions use actual G14 openings, the actual source
ledger and A1 checker. One moves a real Court relation between an accepted
artifact request and its first response. A charge that fits the current wire
cap but would starve four outstanding response units is refused. The other
refuses a missing historical capacity resolution after a source response has
been checked internally and verifies that no partial state is published.

## Verified scope

| Suite | Result | Evidence |
| --- | --- | --- |
| A4/A6 actual-checker/source/real-DB replay target | 40 passed, no ignored tests, default stack | [Final source run](source-court-integration-functional-r3.json) / [raw log](source-court-integration-functional-r3.log) |
| Existing source ledger/geometry, Positive fence and old TIR fence | 40 + 8 + 9 passed | [Regression record](regressions-final.json) / [raw log](source-and-old-fences-final.log) |
| Common contract, including four reservation boundary/oracle cases | 55 passed | [Raw log](contract-accounting-final.log) |
| Accounting and independent entitlement/conservation regressions | 53 passed | [Raw log](contract-accounting-final.log) |
| Contract all-target strict Clippy and Core library strict Clippy | Both passed | [Contract log](contract-clippy-final.log) / [Core log](core-clippy-final.log) |
| Exec A4 integration target strict Clippy | Passed after three existing test-only style fixes | [Final record](exec-integration-clippy-final.json) / [raw log](exec-integration-clippy-final.log) |

The four reservation tests include 3,375 small independent u128-bound cases,
exact/one-below limits, all three resource dimensions, overflow, preservation of
other signed caps and unchanged consumed counters. They are included in the
55 contract tests, not additional runtime executions to add to that count.

## Preserved failures and build conditions

The first A4 filter used the source filename instead of its Rust module name,
selecting zero tests. That build is explicitly **NOT_RUN** in
[its original record](source-court-integration.json). The first full 40-case
run preceded the post-event-origin assertion; it remains separately recorded
and is not added to the final 40-case count.

The first direct-Core regression build stopped at the 512 MiB disk floor,
before tests ran. A subsequent real run exposed the old fence test's obsolete
assumption that court version 2 was unsupported. RFC-0043 and the unchanged
production validator explicitly support private court2. The test now checks
that case and rejects unknown versions 0, 3 and 65535. Production fence logic,
public presets and wire identities were not changed. Both the stopped build
and failed 8/1 run remain in `regressions.json` / `regressions-retry.json` and
their original logs. Successful corrected results are separately recorded.

All builds used the existing shared target, incremental compilation disabled,
existing native archives and a C++-compile refusal guard. Earlier checks used
at most two build jobs. The final source run uses **one job and debug info 0**
to fit available disk space. Cargo's actual selected artifact reports opt-level
0, debug assertions and overflow checks enabled. It ran with default stack and
without global Rust flags. This is functional validation, not performance/RSS
qualification. The final manifest records source hashes before/after (equal),
raw output and the linked executable hash. Its `functional_build_overrides`
override the inherited environment map; effective build jobs are 1.

The first strict Exec lint found three existing test-only style issues; its
failed raw output is retained. The post-lint full-debug build stopped at the
disk floor before tests. An initial compact-build launcher put Cargo's JSON
argument before its subcommand and ran no tests. The next compact attempt
stopped before a selected test artifact; its exact exit was not recorded by
that runner and is explicitly unknown. These failures and incomplete runs are
preserved separately and are not counted as passing runtime checks.

This increment's regenerable intermediates were released with exact path,
size and SHA records (`own-*.json`). Completed object files were matched to
retained library members. A previously successful pre-lint executable was
compressed with full restore/hash verification; see
[a4-before-lint-executable.json](a4-before-lint-executable.json).
An older unused 1.2 GB Core library was also compressed outside Cargo output,
with the full decompressed stream's size/hash verified before cache release;
[its preservation record](old-core-library-compressed-preservation.json)
identifies both paths and hashes. Source, models, native archives and raw
evidence were retained. No Rust library was manually restored into Cargo
output directories. Ordinary Cargo rebuilds its own missing intermediates.

## Remaining full-goal work

This completes the selected local replay increment. It does not complete the
full seven-stage objective, actual native Assignment/Result roles/funding/P,
all production credit consumers, full node restart/reorg/IBD/pruning, or trained
Panel/Node measurements and activation. See
[the complete requirement ledger](../COMPLETION-REQUIREMENTS.md). The existing
native-assignment RFC/ADR adoption question remains pending.
