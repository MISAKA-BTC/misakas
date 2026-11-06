# A1 production port: one pure checker in consensus-core

2026-10-31. This is a placement change for the existing initial-profile checker,
not an expanded derivation, a new codec, or native/public activation.
Source commit: `3b26100c0`; separate module/dependency registration: `900058dec`.
A0 already has the shared-contract dependency and can omit that duplicate line.

## Production paths and dependency direction

The implementation now lives in `kaspa_consensus_core`:

```rust,ignore
use kaspa_consensus_core::{
    palw_positive_check_v2::{CoveragePlan, ExactChecker},
    palw_positive_contract_v2::{PreparedCheck, TirNodeChecker},
    palw_positive_contract_v2::record::{RecordDeliveryError, RecordLimits},
};
```

The old `misaka_palw_tir_exec::node::positive_check` and
`::positive_contract` paths re-export these exact core modules. Opaque context,
progress and terminal types are the same types, not compatible copies. Existing
A3/Node consumers keep their imports. Primitive records use the same A0
`ExecutionRecord` codec through `PreparedCheck::authenticate_record`; the retained
native class/job, bounded generated buffer, delivery-only errors and retry
semantics are unchanged. A0's record source was byte-identical at migration.

Core depends on the shared contract and the existing reference TIR leaf crate.
It has no dependency on tir-exec, mapped artifacts, runtime plans, file-backed
weights, or the executor's thread pool. Production consumers can pass
`TirNodeChecker` through the shared `NodeChecker` port without linking tir-exec.
The exec crate stays a node runtime, as ADR-0042 Decision 4 requires.

The dependency report conservatively follows all normal/build workspace edges,
including optional edges, and excludes dev-only edges. It confirms neither core
nor consensus reaches tir-exec. The pre-existing consensus -> base0 -> artifact
path is recorded separately; this change does not introduce or remove it.
Unfiltered full offline Cargo metadata needs uncached `android-tzdata`; the report
therefore uses `cargo metadata --offline --no-deps` and describes only workspace
edges. The successful production core check independently resolves its real build.

## Mechanical migration and Court composition

All four moved modules match the committed source at `a5f116128` after import
namespace changes and rustfmt. The only additive API is the read-only
`CheckContext::witness_id()` accessor A4's Court adapter already needs. It returns
the authenticated witness identity and grants no computation/native authority.
No arithmetic, leaf order, budgets, attribution, framing or record validation
changed. A separate read-only agent confirmed the normalized comparison.

The expanded plan's pure geometry module moved with its parent and remains
hidden/experimental. The current checker still refuses original sparse/wide
programs. No source-only payload, expanded derivation ID, receipt or activation
is assigned by this relocation.

A4's existing `TirCourtCheckerV2` is a thin delegate, not a second checker. A4 was
asked to move it beside `CourtCheckerV2` in core and import the two production paths
above. Its old exec adapter can re-export that concrete core adapter. A4 owns
the coordinator trait, admission and real event-176 driver; those evolving files
are deliberately not copied into this A1 branch. A0 owns their integration and
shared module/dependency registration. This handoff does not claim the full
event-176 native driver has passed on this A1 candidate.

## Verification

`validation/production-port/` retains the production core typecheck, existing
behavioral regression, path-identity tests, dependency report, normalized source
comparison and final candidate/source hashes. The three new path tests constrain
the exact `NodeChecker` associated types and cross-assign record/plan types;
separate duplicate implementations would fail those checks. Behavioral fixtures
remain the existing actual signed-material tests rather than duplicate fixtures.

The migrated candidate passes **39 tests**: 36 existing behavioral fixtures and
three path/port identity tests, with no failures. Production `core --lib` checks
pass. Strict core-only Clippy still reports exactly the two previously recorded,
unchanged findings: `clone_on_copy` in `palw_model_panel_one_v1.rs:92` and
`nonminimal_bool` in `palw_state_v2/chain_state.rs:3728`. It is not reported as a
strict Clippy pass. Documentation passes no-new-regressions with 43 known missing
references and zero new issues.
Node compatibility library plus the new port test pass package-only Clippy with
`--no-deps -- -D warnings`; this does not override the core-wide result above.

The earlier phase's 43-test evidence stays unchanged under `validation/next-stage/`.
Counts from separate runs are not added together. Total Node RSS, native duty,
funding, credit and public activation remain outside this placement change.
