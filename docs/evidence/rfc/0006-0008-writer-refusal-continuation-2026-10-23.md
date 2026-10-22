# RFC-0006 / RFC-0008 writer refusal continuation

Base: `6a128eb65775c9cf1a26f975b7028c2d4a8b1187`; branch
`codex/rfc06-12-continuation`.

`3c70b4078` propagates `write_kernel_row` failures from reader/router V2,
selected-source bootstrap, V6 paid bootstrap, and the two material-challenge
writers in `transition_builder.rs`. Previously these five consumers discarded
the `Result`, although the writer can refuse local resource admission before
changing its map or journal.

`b9ba2e757` propagates both the provider receipt-stream checkpoint write and
the material row write from `routed_answer_v2`. It also routes the common
monetary material row writer through `write_kernel_row_encoded_v1`, which
reserves the actual key and canonical row buffers before allocating them.
These changes retain `NativeResource` as a local retryable error rather than
continuing a transition with a missing write. They do not enable a fence.

## Verification boundary

The baseline Rust test started before these writer changes completed:

```text
CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-testnet/target cargo test \
  -p kaspa-consensus-core --test rfc0012_native_evidence_fold -- --nocapture
test result: ok. 6 passed; 0 failed; 0 ignored
```

This verifies the earlier tree's fold evidence extraction, retirement retention,
and post-Final conviction retraction. The fixture drives a floor claim and
explicitly relabels its record for part of the evidence-reader exercise; it
does not establish an actual registered-model useful-work campaign. It is
also not a regression run of the new writer changes.

The campaign driver control suite separately passed 14 tests. After both writer
commits, `cargo check -p kaspa-consensus-core --lib` exited 0 (3m 40s, 385
warnings). Its output is at `/tmp/misaka-rfc06-12-writer-check.log`. No unused
`Result` warning remained in that check. This is compilation evidence, not a
runtime capacity-refusal test. `git diff --check` and the documentation
no-new-regressions check succeed; the documentation baseline still has 43
known issues.

Still required: targeted capacity-refusal controls at the outer consumers,
restart/reorg/prune and physical resource acceptance, the full model/source/
adapter/payment requirements, and the 15-gate real-process campaign. No gate
is promoted from UNKNOWN by these checks. Public presets and the unarmed
full-activation height remain unchanged.

## Runtime continuation at `a2fcee238`

The reader/router consumer's new late-refusal regression passed: it measures
the concrete successful write's admitted request, then supplies one byte less.
The consumer returns `NativeResource`, leaves state and journal unchanged,
and releases all reservations when the builder drops. This checks the outer
consumer that previously discarded the writer's result; other outer consumers
still require their own controls.

The library test build also exposed and repaired two missing closing braces
in pre-existing post-Final duty/material intake tests and two EVM market
fixtures missing the dormant selected-source bootstrap fields.

The subsequent filtered library run passed 41 tests covering native writers,
GDN court, settlement-once and liability. Logs:
`/tmp/misaka-rfc06-router-capacity-test.log` and
`/tmp/misaka-rfc06-08-core-consumers-test.log`.

A further material/router/bootstrap/retention/liability run passed 40 tests
and failed one stale wire-tag assertion. The actual declaration codec and
lifecycle registry assign `ReaderRouterDeclaredV2` tag 248; 244 belongs to
`MemoryRoutingV2`. `fc88a4a38` corrects only the assertion. The combined
recheck passed 81 tests with zero failures, using the actual library test
binary rebuilt after that correction. Output is in
`/tmp/misaka-rfc06-08-consumers-recheck.log`.

Source inspection also identified a remaining production GDN history issue:
`gdn_filer_tick.rs` discards both errors and continuation from its 4096-block
accepted-lifecycle page read. This is not runtime evidence of a false verdict,
but completeness of disclosure lookup across the full eligible history has
not been established. Production GDN acceptance must include paged history,
read failure, restart and reorg cases rather than treating a prefix as complete.

`29b97afcd` now propagates a page-read error out of the blocking collector:
the service logs it and returns before stepping the filer book or queuing a
new court object. This fixes the error path only. Continuation beyond the first
page remains unimplemented. `cargo check -p kaspad --lib` passed for that
change (2m 36s; 60 warnings), with output at
`/tmp/misaka-rfc06-gdn-service-check.log`.

`dbf505cae` extracts the production strict page collector and adds a control
using the accepted-chain mock: a short page reports its continuation; an
unavailable older acceptance record returns an error rather than a partial
success. The accepted-history/service tests are compiling with output at
`/tmp/misaka-rfc06-gdn-history-tests.log`. The new test's outcome is not yet
known. Cross-tick continuation, runtime failure/retry controls and production
acceptance are still required.

## Service/history execution and wider GDN failures

The history/service run subsequently passed all 15 tests, including the new
GDN page error/continuation control. The wider `gdn` filter ran 20 tests:
17 passed and 3 failed (`/tmp/misaka-rfc06-gdn-all-tests.log`). Ordinary signed
registration and host resolution passed. Two held-court E2E cases (answering
forger and silent forger) stopped at claim acceptance with
`ImmutableModelUpdate("V2 claim needs its native canonical profile")`.
These are failed acceptance tests; no conviction/slash success is established
by those two runs. The routing gate reads `fp_work_profile_of`; tracing the
canonical registered-profile source and its consumer connection remains work.
Do not bypass that guard or add model-level review to make the fixture pass.

The third failure expected the fold ladder refusal but attempted to construct
evidence first, producing unrelated unsupported-kernel/coordinate diagnostics.
`757e52a4c` now rejects a served leaf outside the fold's ladder before evidence
allocation/recompute. Its targeted recheck is running with output in
`/tmp/misaka-rfc06-gdn-ladder-recheck.log`. No result is claimed yet.

The targeted ladder recheck subsequently passed (1 test, zero failures,
0.86s). This resolves that observed probe failure only.

Further source tracing of the two E2E failures found that
`stamp_reader_router_claim_v2` reads `fp_work_profile_of`, while the only
production caller of `write_fp_work_profile` is the `ClassLaneCertified`
object arm. Ordinary `ClassRegistered` does not populate that graph index.
The index also feeds free-prompt pricing and class deadline/held-context
calculations, so storing ordinary registration profiles there requires an
audit of those consumers, not merely a fixture insertion. The missing normal
registration-to-routing connection remains open. Model-level verification or
certification must not be added as a workaround (PRINCIPLES section 9).

## Ordinary registration graph retention (`c4201b296`)

Past the existing memory/routing fence, ordinary legacy class registration
retains its carried graph in table 147 under `0xA3 || class_id`. Routing
consumers use that graph, retaining the existing legacy graph fallback without
writing the free-prompt pricing/deadline index. Decode has a native resource
owner; writes use the existing reserved cell writer. Class identity,
immutability, restore consistency and class removal cover the new namespace.
This stores structural registration data and grants neither model approval nor
claim credit. No fence is armed.

`cargo check -p kaspa-consensus-core --lib` passed (15.94s). The targeted
ordinary-registration/profile/delta-reversal test is compiling; output is
`/tmp/misaka-rfc06-registered-profile-test.log`. The two failed GDN E2E paths
must still be rerun after their fixture supplies the native route/declaration
prerequisites; this compilation is not evidence that they now pass.

The concrete aux scanner recognizes the profile row. Its generator source
also has the profile arm, but the base generator still uses the old router
table number 136 while the checked-in scanner uses 147 (and the adjacent
bootstrap namespaces have moved too). Full generator reconciliation and
idempotency remain open; do not overwrite the current scanner with the old
namespace output or claim reproducible generation from this check.

## Generator reconciliation (`48fd931b1`)

An isolated copy reproduced the generator's failure at the old aux134 marker.
The generator now emits material checkpoint namespaces in 137 and routing/
bootstrap namespaces in 147/148/149. Two isolated generations matched. The
three differing generated files were reviewed and installed after the Rust
test process finished; no target types were removed. The inventory now also
includes the selected-source bootstrap fields previously omitted from the
transition-extras visitor.

The official command then exited 0 in the real checkout:

```text
python3 scripts/drills/check_rfc06_native_wire_idempotent.py
idempotent; committed outputs are the generator output
```

Output: `/tmp/misaka-rfc06-generator-idempotent.log`. This closes the observed
generator-numbering/reproducibility gap for this tree.

The first ordinary-registration test run failed before exercising profile
retention because the shared fixture specified slash value 1 whereas its
existing floor specifies 5. The fixture now reads that value from its floor
row rather than changing any network parameter. The runtime recheck is
compiling; output: `/tmp/misaka-rfc06-registered-profile-recheck.log`.

The corrected registration test passed (1 test). A further run of native
writer, reader/router, manual wire and aux registry controls passed 26 tests
(`/tmp/misaka-rfc06-profile-wire-controls.log`). These are local library/fold
controls; they do not establish a model-wide cost measurement or real IBD.

`5452a3186` supplies the existing GDN fold E2E fixture with a native kernel
route and an explicit per-reader declaration authored from the fixture graph.
It does not insert a profile directly into state, enable model certification,
or guess a router in production. The two GDN E2E cases are rebuilding with
output at `/tmp/misaka-rfc06-gdn-e2e-routing-recheck.log`.

`f2cf3663a` additionally checks native carriage encode/decode/restore at the
expected state root in the ordinary-registration test. That expanded test is
queued with output at `/tmp/misaka-rfc06-profile-snapshot-recheck.log`; its
outcome and the GDN E2E outcome are not yet known.

The `5452a3186` GDN E2E recheck subsequently failed both cases at registry
construction with `routing V2 journal requires native kernel route`. The
fixture supplied route policy, but the route's first header had only been
initialized by the later closing tick. `2ffe0ef32` calls the existing reserved
`ensure_route_header` on first profile retention when no route exists, before
writing the graph. Existing routes and pre-fence registration retain their
existing path. The ordinary-registration test now starts with no native route
and supplies route policy through extras, so its delta/restore assertions also
cover this first-use boundary.

This latest test is queued at `/tmp/misaka-rfc06-first-route-profile-recheck.log`.
The earlier snapshot run at `/tmp/misaka-rfc06-profile-snapshot-recheck.log`
was already live and is allowed to finish; its result must not be attributed
to the first-use fix. Neither GDN E2E case has passed yet.

The earlier snapshot run subsequently passed (1 test, zero failures) after a
7m17s library-test build. It verifies retained-profile native carriage restore
for an already initialized route, in addition to delta apply/revert and the
unchanged price/deadline/certification indexes. It does not verify first-use
route initialization.

The revised first-use regression is now compiling under the original handle.
The GDN E2E recheck of `2ffe0ef32` is queued separately at
`/tmp/misaka-rfc06-gdn-first-route-recheck.log`; neither of these later runs
has produced an outcome yet. Earlier failing GDN logs are preserved.
