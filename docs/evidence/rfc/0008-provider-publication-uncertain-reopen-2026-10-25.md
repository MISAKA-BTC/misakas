# RFC-0008 provider snapshot publication and crash recovery

Status: implementation and coordinated retry4 verification PASS; scope limits below remain.

## Actual failure and consumer contract

The durable provider wrote and fsynced a candidate snapshot, renamed it over the prior snapshot, then fsynced the directory. It replaced its in-memory retained store only after all these operations succeeded. A rename followed by directory-fsync failure therefore left a new on-disk generation and an old in-memory generation in the same live provider. Returning an error did not prevent later answers, admissions, GC, metadata retirement or ACK signing from using that old projection. The routed snapshot publication path had the same boundary.

Both raw and routed snapshot publications now share the filesystem publication helper. Pre-rename write/file-fsync failures preserve the usable old generation. Immediately before the rename attempt, the owner marks its publication outcome uncertain; only a completed directory-fsync barrier clears that state, after which the caller replaces its in-memory candidate. A failed rename is conservatively treated as uncertain too. Recovery requires dropping the provider and reopening the actual persisted snapshot under its exclusive process lease, using the existing full codec/identity/capacity checks. No failed publication authorizes a liability signature.

All public material preparation, signature-context persistence, response, current-generation replay, release and expiry mutation paths check storage health before their work. The private provider remains unavailable after an uncertain publication until it is closed and reopened; this change does not introduce automatic service restart. The diagnostic held-set accessors retain their signatures; they do not authenticate the persisted state after an uncertain publication. The janitor checks before intent read/replacement, between contracts and before terminal journal publication, so an uncertain owner cannot overwrite the saved deletion tuple. The actual private provider GC caller checks again after expiry/janitor operations and before using diagnostic held sets to retire authority metadata or enqueue a signed recovery ACK. This second check covers a failure arising during the same tick.

Immutable routed authority sidecars still use their existing exact-record identity check. They are not serving authority or a substitute snapshot generation. Metadata cleanup after a successful snapshot remains a separately retryable operation; a cleanup error does not by itself create a disk/memory snapshot mismatch.

## Already-published deletion and a temporarily revoked witness

A second crash/restart defect was found while exercising the real janitor WAL. `delete` can correctly report false when a reopened snapshot already lacks the raw consent but the current witness refuses its saved tuple. The replay caller assumed false always meant all bytes still existed, removed the intent, then discarded the watch because the raw consent was absent. This lost the only durable cleanup tuple/native-claim inventory before metadata/ACK completion.

Replay now cancels an unexecuted candidate only while the consent is still physically held. If raw deletion has already published, a false witness preserves the exact intent and stops this tick without completion. A later positive witness replays that same tuple. Unknown reads also retain it. This is recovery bookkeeping, not permission to delete on physical absence or to override a native witness.

## Prepared controls

Remote library filter `provider_publication_` contains four parents:

- Actual file-fsync/pre-rename error preserves byte-identical snapshot and live answers; retry uses the real GC mutator, then cold reopen verifies the remaining exact consent.
- Actual rename followed by an injected error before directory fsync changes disk while leaving the old projection. Both raw and routed publication doors must refuse answers, native-authority callbacks, subsequent publication, GC and janitor intent handling until reopen. The existing intent bytes remain unchanged. Cold codec-checked recovery observes exactly the published remaining consent and can answer it; the removed consent cannot answer.
- Unix subprocesses stop at actual publication checkpoints after candidate file fsync, once before rename and once after rename. The parent verifies the real flock excludes another opener, SIGKILLs the child, checks signal9 (no Rust destructor/clean shutdown), then cold opens the exact old/new atomic generation. The other consent remains answerable in both cases; only the uncommitted pending snapshot is cleaned under the reopened lease. A second cold open checks repeatability. Children are inclusive in their parent count.

- The real janitor observes a held contract, discovers its exact release tuple, fsyncs its actual intent and encounters the post-rename barrier error. It stops without a false completed journal. Cold reopen finds raw bytes gone; a negative witness preserves the saved intent and produces no completion, then a positive witness completes exactly once. The completion receipt/native-claim inventory survives a further journal reopen.

The fixture's kernel claim/material is genuine small deterministic kernel execution and codec-authenticated storage. Its finality callback is explicit test input, not a native accepted Release or a provider-host campaign. Injected I/O errors control the production filesystem boundary; they are not actual hardware faults. SIGKILL is process termination, not OS-kernel crash, power-loss/fsync correctness, 100k work or K3 evidence. Existing actual native terminal/replacement/GC fixtures will remain a separate verification scope.

No activation, economic policy, model tier, native39 settlement mapping, selected-verifier approval requirement or wire identity is changed. K3 inputs remain uncreated, formal native39 mapping unformulated, and real campaign gates UNKNOWN.

## Initial verification checkpoint

The combined retry2 five-package no-run build passed in 146.78 seconds with all 31 pre-build source/config identities matched. The dedicated remote filter initially passed three parents and failed one at an incorrect fixture assertion: this kernel-codec storage fixture has no native-held authority IDs, so asserting a nonempty inventory was not legitimate. The corrected test compares the saved receipt to the actual pre-deletion inventory (empty here), without creating or approving a native claim. Nonempty native-held inventory coverage is not inferred from this control. The exact Release tuple and actual WAL/reopen/witness behavior remain mandatory. Initial failed runtime/logs and pins are preserved at `/tmp/misaka-rfc-payout-provider-market-focused-results-retry2-20261025.json` and `/tmp/misaka-rfc-payout-provider-market-pins-retry2-20261025.json`.

Retry3 is PENDING. Production provider code did not change after retry2; only this fixture expectation was corrected. Separate EVM comment corrections explain the changed local/invalid error contract. No initial failure is counted as a PASS.

## Final coordinated checkpoint

Retry4 compiled with 33 identities matching and all 14 dedicated parents passed. All 13 related groups passed, with 375 distinct passing parents across overlapping selections; four ignored parents are excluded. Private-node and non-EVM checks passed before only the two Remote test-only repairs. Earlier pending statements describe earlier checkpoints. [Final pins, failures, counts and scope](0006-0008-0012-payout-provider-market-verification-2026-10-25.md).
