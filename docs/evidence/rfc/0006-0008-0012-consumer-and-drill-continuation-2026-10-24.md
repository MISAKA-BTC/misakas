# RFC06/07/08/12 consumer and drill continuation — 2026-10-24

This follows the [parallel implementation record](0001-0038-parallel-continuation-2026-10-24.md).
The previous goal turn made verified implementation progress. The active goal still includes every
requirement in the [remaining-work ledger](../../design/palw/integration/rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md);
this continuation does not replace that goal with focused test success.

## Actual consumer changes

- **RFC06/08 court arrays:** `CourtOutboxV1::try_push_clone` admits its pending array growth before
  allocation. The previous frame reservation stays live through a failed or successful growth attempt.
  The carrier prepares a bounded, preallocated `CourtRetryBufferV1` before draining, funding or submitting
  anything. Refusal leaves the queue intact for a later tick. Each original item can enter the retry buffer
  at most once, so pushes cannot grow past the admitted capacity. The unsent array transfers its ticket
  back into the outbox; an empty Vec still retains its ticket while it retains allocated capacity.
  The original batch's array ticket remains in the outbox until the loop consumes/drops that batch.
  A canceled flush is cleaned up conservatively on the next flush. Disabled-capacity fixture extraction
  accepts owners without live leases, while strict extraction cannot detach a live ticket.
- **RFC07 private V6 receipt:** the provider action signer now accepts the `ReceiptApprovedBootstrapV6`
  namespace that authenticated private sources already generate. It retains the existing actor check,
  exact frame, network domain and signing context. See the [signing evidence](0007-provider-v6-receipt-signing-2026-10-24.md).
- **RFC08 durable spends:** an ACK or additional-carrier WAL cannot replace a saved funded transaction
  with another spend of the same signed object. Identical retries compare canonical Borsh bytes through a
  borrowed streaming writer, perform no file publication and preserve the ACK inclusion hint. Cold reopen
  preserves the same rule. Explicit release-only proposal replacement remains available through its existing
  API; native finality remains the authority for retirement.
- **RFC12 finite fixture ownership:** commands launch in a fresh POSIX session. Timeout, sampler-registration
  failure and leader exit all clean up the owned process group and reap the direct child. Cleanup is recorded,
  and final output size is checked after cleanup. See the [process lifecycle evidence](0012-owned-fixture-process-groups-2026-10-24.md).

## Verification

- Actual court outbox source plus the unchanged actual service EDF function: **5 test functions passed**
  (0.01 seconds), including an isolated quota subprocess. It covers refusal without drain, pending and
  retry array retention after payload removal, capacity release, duplicate payloads, cancellation/retry,
  disabled-capacity fixture extraction and existing EDF/debounce parity. This is a focused source harness,
  not the entire async service or full-node execution.
- Actual SDK crate: **5 provider-action tests passed** (0.13 seconds), including network/namespace
  isolation and a different actor being refused before signing.
- Actual remote crate: **2 ACK tests and 4 carrier-queue tests passed** (0.06 / 0.07 seconds), including
  real filesystem publication/reopen, immutable funding, preserved inclusion hints, finality deferral and
  explicit release-proposal replacement.
- Actual Python subprocesses: **6 new lifecycle tests passed** (0.637 seconds); existing multiprocess
  fixture **11** and owned-resource **4** tests also passed.
- Full private kaspad library and its test targets passed metadata/type checking (25.21 seconds). This is distinct from
  runtime coverage and does not mean the node/campaign tests ran.

Local evidence paths: `/tmp/misaka-court-frames-tests.log`, `/tmp/misaka-court-frames-build.log`,
`/tmp/misaka-court-frames-kaspad-check.log`, `/tmp/misaka-court-frames-manifest.json`, `/tmp/misaka-rfc07-provider-signing-tests.jsonl`,
`/tmp/misaka-rfc08-immutable-ack-20261024.log`, `/tmp/misaka-rfc08-immutable-carrier-manifest-20261024.json`,
`/tmp/misaka-rfc12-owned-fixture-20261024.log`,
`/tmp/misaka-rfc12-multiprocess-fixture-regression-20261024.log` and
`/tmp/misaka-rfc12-owned-resources-fixture-regression-20261024.log`.

## Remaining boundaries

Raw legacy queue construction/enqueues can still allocate outside the admitted copy path. The new array
owner does not retroactively authorize their earlier construction or any later raw growth. EDF scratch,
deadline/submission maps, carrier encodings, other court/payout/aux consumers, input/cache allocations,
whole-model costs and complete historical-header consumers remain open. Typed memory requests are not physical RSS measurements or measured all-model cost bounds.

A WAL exact-byte retry is not chain acceptance, correct funding, finality, paid-once reward settlement or
complete material recovery through restart/reorg/prune. The process-group cleanup does not contain a
child that deliberately creates another session, enforce an OS quota, or measure every descendant's RSS.
Real multiprocess work, 100k, pruned IBD, OS-crash campaign, external adapters and historical signatures
remain subject to their actual evidence requirements. No UNKNOWN gate is promoted to PASS.

Activation remains `None`. Public selected-source/V6 bootstrap refusal and private-only unapproved
source economics are unchanged. K3 formal specification, manifest, weights and signed inputs remain
uncreated as reported by the user; no toy fixture substitutes for them. Model tiers, claim-level selected
verifier approval and the prohibition on cryptographic model-computation correctness proofs are retained.
