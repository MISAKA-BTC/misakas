# RFC-0006 native claim court-membership construction owner

Status: implementation and bounded native runtime controls **PASS** on 2026-10-25. Six-package build and immutable binary verification completed. No new activation, policy, wire field, approval rule or model tier is introduced.

## Actual consumers and ownership

`palw_state_v2.rs::apply_attempt` now uses the mutable native court admission wrapper before its first write. `palw_funded_original_coordinator_fold_v1.rs::install_original_admission_v1` uses the same wrapper immediately after its escrow install, within the existing paired-original checkpoint. Capacity and ingress decisions retain their original order, including below-fence absence and current-claim exclusion. The raw helpers share the exact bucket predicate and remain compatibility APIs.

The formerly raw funded Set collected all open, unsettled escrow claim IDs. The native wrapper first counts these borrowed rows, F, then admits F concrete `BTreeMap<Hash64, ()>` node envelopes through `NativeWriteQuoteV1::member`. It inserts sequentially; D distinct members satisfy D <= F, including duplicate escrows for one claim. This avoids the bulk collector's temporary sort/storage. Zero qualifying rows require zero heap/no quota. The construction lease remains in the transition builder after the local Set drops; it does not replace keyed writer/journal or checkpoint reservations. No serialized-byte ratio, model dimension guess or approved court cost is claimed.

The count-only paid-ingress decision now directly counts the existing borrowed predicate, avoiding its previous collecting/sorting Vec for every raw and native caller. The collector remains for callers actually requiring admission order.

`attach_riders_v1` previously converted every `apply_attempt` error to `CapacityRiders(String)`. It now forwards the existing `NativeResource` and `NativeContextUnavailable` variants unchanged; semantic errors retain the existing rider-indexed text. Consequently `apply_pending_riders_v1` can propagate local failure rather than publish a semantic skipped-rider outcome.

## Dedicated controls prepared

Exact filter: `palw_state_v2::tests::claim_court_admission_workspace_v2` (three parents; all executed and PASS).

1. Actual `apply_attempt` and the coordinator's funded-admission phase refuse at 0 / Q-1 before a state/journal change, retry, keep Q until builder drop, and compare untracked/tracked state, root and journal bytes after a successful attempt. Q is Set scratch, not the full write budget.
2. F/D duplicate input, closed liability, F=0, dormant zero quota, and old collecting/sorting count parity over exclude/nonexclude, exact-cap, inclusive hard deadline and reservable/terminal phases. Additional rows in the count projection are stored test rows, not authenticated fresh admissions.
3. Actual `apply_pending_riders_v1` executes checkpoint and lead writes. A cfg(test) read-only observation records outstanding capacity immediately before the real Set reservation. A fresh run sets its checker limit before the operation to prior+Q-1, must reach that same phase, and must return typed Resource with no semantic skip/new rider. The hook cannot alter state, capacity, requests or return an error. Dropping failed construction releases its owner; the parent input remains immutable. NativeContext forwarding also has a direct variant control, which is not an independent actual missing-context campaign.

The fixture uses existing registry/attempt state-fold helpers and a private placeholder capacity policy. Stored open escrow rows exercise membership. This does **not** establish fresh signed funded-original object acceptance/whole paired-coordinator rollback, production policy approval, stateless signature verification, public arming, all-model cost, K3, RSS or a multi-process campaign. Those remain separate acceptance work.

## Verification and remaining boundaries

Static parse of changed existing files uses rustfmt stdout with `skip_children=true`; only new leaves are formatted. `git diff --check` and the documentation no-new-regressions check are required before freeze. The checks passed with the existing documentation baseline unchanged.

The pure producer precheck still uses the raw Set helper; its existing String boundary in the acceptance walk is not silently counted as typed resource coverage. This patch covers mutable native attempt/funded-install consumers and rider propagation only. It does not close every payout/court/aux caller or the RFC-0006 full-model cost requirement. Full activation stays unarmed; selected-verifier approval and ordinary bootstrap refusals are unchanged.

## Executed evidence

The six-package no-run build succeeded in 173.12 seconds with `kaspa-consensus/evm,kaspad/evm,kaspad/private-integration-config`, test debug disabled, incremental disabled, and the consensus-core test optimization override set to zero. The exact command, source/config identities and identity recheck are preserved in `/tmp/misaka-rfc-claim-provider-native-wave-source-retry4-20261025/record.json`; build log: `/tmp/misaka-rfc-claim-provider-native-wave-build-retry4-20261025.log`.

Six executable identities were pinned before later source edits in `/tmp/misaka-rfc-claim-provider-native-pins-retry4-20261025.json`. New controls: **3 PASS, 0 failed, 0 ignored**, log `/tmp/misaka-rfc-claim-provider-native-core-runtime-retry4-20261025.log`. Related nonoverlapping controls: **21 PASS, 0 failed, 0 ignored**, comprising admission workspace 3, capacity workspace 3, Legacy ingress 3, deadline workspace 2, funded escrow 6, paid ingress 2 and funded admission 2. Commands, exact binary identity and logs are indexed in `/tmp/misaka-rfc-claim-court-related-results-retry4-20261025.json`. Total for this core scope is 24 distinct tests; provider subprocess results are separate.

Earlier attempts are retained: the initial build exposed a borrow lifetime error; retry1 exposed a non-Send fixture future; retry2/retry3 core runs failed because the rider fixture gave its lead zero worker carve. Retry4 uses the real 620-permille worker carve and asserts positive native lead escrow before reaching the target phase. It does not weaken the production rider refusal. These failures and corrections are not counted as passing acceptance. Temporary paths are local evidence and must be retained/exported with the release record before cleanup.

The [durable evidence index](0006-native-claim-court-membership-owner-2026-10-25.json) embeds the build/source identities, six executable hashes, commands and all core runtime logs.
