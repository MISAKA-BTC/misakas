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
