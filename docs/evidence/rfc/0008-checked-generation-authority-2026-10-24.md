# RFC-0008 checked generation authority — 2026-10-24

## Contract

The current material API now consumes
`ConsensusApi::palw_material_generation_authority_checked_v4`. Its typed result
separates a candidate-specific `Denied` from local `Resource`,
`CapacityUnavailable`, and native `Unavailable`. Only an initial `Denied` can
select another retained consent. Capacity, missing/unreadable native context,
codec faults and all failed post-assembly checks stop the read.

The production consensus implementation preserves candidate reader
`NativeCapacityUnavailable` without inventing the quota detail that reader no
longer exposes. The existing pruning-import-status reader has already reduced
its underlying store cause to an unavailable String; this path stops the read
as `Unavailable` without guessing a capacity classification. State consistency
failures preserve `NativeResource` and native
context capacity variants. Other state/import/point failures are unavailable,
not a denial of the requested material. Current root/provider/scope and enrolled
consent checks remain in the same core authority helper.

The checked row decoder validates wire shape before its reserving classified
decode. A malformed row is unavailable state; a failed allocation reservation
is a typed resource refusal. Neither is eligible-consent absence.

The old native and core `String` functions remain wrappers. Public remote answer
signatures remain unchanged and convert the checked cause to text only after
making the fallback decision. The checked trait default returns `Unavailable`;
it cannot safely infer a typed cause from an implementation that overrides only
the historical String method. Such older implementations remain usable through
the historical API but must implement checked authority to serve current V4
reads. No error-message matching is used for the decision.

This closes the initial generation-authority classification limitation recorded
in `0008-current-generation-answer-fence-2026-10-24.md`. Kernel context APIs and
other String APIs are not globally migrated. There are no new activation heights,
policies, model tiers, source economics or wire formats.

## Verification scope

Focused remote regressions use the actual admitted/persisted/cold-reopened
provider store and response owner with injected authority outcomes. They test
that initial `Resource`, `CapacityUnavailable` and `Unavailable` stop at the
preferred candidate; initial `Denied` alone can fall back. Existing exact-consent,
post-assembly generation change and actual logical-capacity/owner-release cases
remain in the same group.

A core child-process regression exercises the exact generic row decoder used by
checked authority, with a scalar fixture isolating wire and reservation behavior:
zero logical capacity refuses a valid row as `Resource`; a truncated row remains
`Unavailable` under the same limit. This is not a native replacement/reorg,
multi-host provider or K3 acceptance campaign.

The first parallel focused run passed 3 of 4 cases but hit a fixture lease
collision while opening the fourth provider. Fixture directory names now include
a process-local atomic sequence in addition to PID and timestamp. The production
provider lease behavior was not weakened. The original failed run is retained in
`/tmp/misaka-rfc08-checked-generation-remote-tests-20261024.log`.

The corrected focused remote run passed 4 cases plus its isolated child in
0.26 seconds. Its resulting `misaka_palw_remote-99cd4a5f4ba5f9b4` binary passed
all 15 `monetary_material` cases in 0.38 seconds. The core cargo run passed the
new decoder regression plus its isolated child in 0.01 seconds. Existing native
provider-generation/replacement/cold-route/rollback and provider-capacity/GC
regressions also passed against the core binary containing the checked helper.
Build durations include shared artifact-lock waits (focused remote 10m26s,
core 11m33s); they are not execution-cost measurements.

Run logs are `/tmp/misaka-rfc08-checked-generation-remote-tests-final-20261024.log`,
`/tmp/misaka-rfc08-checked-generation-all-tests-20261024.log`,
`/tmp/misaka-rfc08-checked-generation-core-tests-20261024.log`,
`/tmp/misaka-rfc08-checked-generation-existing-query-20261024.log` and
`/tmp/misaka-rfc08-checked-generation-existing-capacity-20261024.log`.

`cargo check -p kaspad --lib --tests --features private-integration-config` passed,
compiling the production checked authority override, the remote consumer and
private recovery/query callers together. Its 11m48s duration includes shared
build-lock waits. This is metadata/type verification, not a native node campaign.
The log is `/tmp/misaka-rfc08-checked-generation-kaspad-check-20261024.log`.
