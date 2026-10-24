# RFC-0007 typed Memory provider entry-state fallback — 2026-10-24

## Repaired production connection

The production private checker calls `VerifiedPaidClaimSourcesV2::verify_using_provider_v4` after
reacquiring its own assignment/source. That method authenticates the canonical opening and dispatches
the all-target provider reader. Its TypedSpec branch invokes the actual seeded `SpecOutsiderV1` checker.

For a Memory claim starting at its initial state, the typed checker first tries to read M0 from the
artifact. When unavailable there, it falls back to the authenticated entry-state position at stage
`0x40`. The complete provider codec intentionally excludes mutable Memory slots from immutable
parameter rows: entry state and subsequent writes carry those values instead.

`Reader::param` treated that expected optional absence as a sticky acquisition error. The fallback
could obtain and authenticate the correct entry state, and the checker could finish every step, but
`check_provider_v4` rejected its result because the earlier optional lookup had poisoned `Reader.error`.
Thus the real initial-Memory provider path could not yield a completed local check.

The reader now returns a normal optional miss only for an exact mutable slot in the registered
Memory class, under program 0 and the exact parameter/layer pair. That lookup makes no immutable
parameter query. Missing immutable artifacts, incorrect program/layer requests, malformed data and
provider failures retain their existing error behavior. Entry-state authentication is not bypassed.

The public source caller uses the same private `check_provider_material_v4` execution function as the
regressions. It accepts a `ReservedKernelLedgerV1` owner, reserves material before reader/checker
construction, dispatches all four existing target types and preserves the sticky-error refusal for
actual acquisition failures. No new public constructor bypasses source authentication.

## Verification

The actual SDK crate's provider-reader tests passed: **7/7**, including four new Memory controls and
three existing parameter/commitment controls (3m41s build including a shared-target lock wait,
1.64 seconds runtime):

```sh
TMPDIR=/private/tmp RUST_MIN_STACK=67108864 \
CARGO_TARGET_DIR=/tmp/misaka-rfc06-12-build-20261024 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo --config 'profile.test.package.kaspa-consensus-core.opt-level=0' \
  test -p misaka-palw-sdk --lib claim_paid_v2::provider_reader_v4 \
  --message-format=json -- --nocapture
```

The Memory fixture registers an existing memory sketch through the kernel ledger API, posts a real
two-step job, runs `produce_native_work_v3`, and accepts the actual salted seal/reveal through that
ledger API. It reconstructs provider units from the resulting material and authenticates every unit
with the actual complete-codec classifier. The checker then rebuilds a reserved native ledger from
its rows and runs the same provider execution function used by the public authenticated-source caller.

- An initial claim with no carried predecessor succeeds through the authenticated entry-state
  fallback; the seeded checker completes both steps and can construct its local complete receipt draft.
- Missing or malformed entry-state material and a missing immutable parameter each refuse the check.
- A separate malicious-producer control alters a MatMul in each step in turn and commits that
  altered trace. Every provider unit still passes membership authentication. The actual direct checker
  localizes step 0 or step 1, produces no complete result, and its filing convicts through the actual
  bounded kernel ledger court. Membership authentication therefore cannot stand in for computation
  checking.
- Only the exact registered mutable slot is an optional miss. Another program or layer remains a
  sticky acquisition error.

Local runtime output is `/tmp/misaka-rfc07-memory-provider-court.jsonl` and its `.stderr` sibling.
The initial fixture used an obsolete unsalted reveal; native kernel refusal caught that, and the
fixture was corrected to use the existing salted seal/reveal contract before successful verification.

The actual private node caller and its test targets also passed
`cargo check -p kaspad --lib --tests --features private-integration-config` with the dedicated target,
debug information disabled and incremental compilation disabled (7m23s including a shared-target lock
wait). Its local output is `/tmp/misaka-rfc07-memory-provider-node-check.jsonl` and its `.stderr` sibling.
This is type/metadata verification of the source caller, separate from the 7 runtime tests above.
Document checks retain 43 known reference issues with zero new issues.

## Boundaries

The test bootstrap seed/check context is local test input. It does not establish source independence,
create native selected seats, accept a native paid receipt or advance credit. The success tests the
actual material/checker execution and local completeness, not the selected-source authorization layer.
Program/Pipeline/Segmented dispatch is preserved, but this change does not claim their end-to-end
campaigns are complete. No four-adapter campaign, K3 execution or whole-model cost bound is claimed.

Activation stays `None`; ordinary selected-source/V6 bootstrap activation remains refused. Model
tiers, claim-level selected approval and public presets are unchanged.
