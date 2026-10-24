# RFC-0006: native block body admission lifetime (2026-10-25)

Status: implementation saved; core, actual DB, stale-ID, unpermitted-round and node metadata checks passed. This is local logical admission for the owning coinbase render, not whole-block admission, physical RSS calibration, or a campaign gate result.

## Actual consumer change

The previous `BlockTemplate` held its coinbase ticket, but native producers extracted its `MutableBlock`, and the immutable body/store retained only `Arc<Vec<Transaction>>`. The ticket could therefore expire while the render remained in a block or body cache.

`BlockTransactionsV3` pairs the private transaction payload with its local admission owner. Cheap clones of the block, body-store entry, API body result, and mining notification retain both. Native heartbeat, lead, receipt, redemption, round, and execution-slice producers move their template through `into_immutable_v3`. The shared owner metadata is admitted before construction. The native fee-statistics consumer borrows the entire template rather than extracting its vectors and dropping admission early. The actual mergeset consumer borrows the coinbase slice of an unpermitted round while retaining its body, eliminating its old coinbase-only deep copy. A stale transaction-ID repair obtains a separate coinbase copy reservation before copying and finalizing; a no-op repair preserves the original owner. Capacity refusal reaches the native submit path as `LocalNativeCapacityUnavailable`, before consensus insertion.

The persisted body remains the existing transaction-vector representation. A decoded disk or peer body does not receive a persisted admission capability. Restore/decode admission is a separate boundary. Incoming transaction semantics remain the body validator's responsibility; quoting an ID-repair copy does not require its first transaction to be semantically valid.

## Verification

Core build and tests passed (3 tests, plus the isolated child quota worker) in `/tmp/misaka-rfc-body-owner-core-tests-final-20261025.log`. The checks cover byte-identical persisted vector encoding, cheap clone lifetime, no-op repair, pre-copy capacity refusal, and an independently retained stale-ID repair. Test-profile compilation took 3m14s; this is not an economic or model execution measurement.

The first coordinated DB/base0/node build queue was deliberately cancelled before runtime after identifying a prospective-routing tip-progress liveness defect; these interrupted logs are not passing checks (`/tmp/misaka-rfc-body-owner-store-tests-20261025.log`, `/tmp/misaka-rfc-gdn-capture-owner-tests-20261025.log`, `/tmp/misaka-rfc-native-owners-node-check-20261025.log`). This was build cancellation, not an OS crash campaign.

The actual DB/cache/delete/external-owner controls passed (2 tests including the worker entry, with the isolated child doing the capacity assertions), 0.12 s: `/tmp/misaka-rfc-body-owner-store-tests-retry4-20261025.log`. Test compilation took 4m29s. Existing actual stale-ID split controls passed 2/2, 0.25 s: `/tmp/misaka-rfc-body-owner-stale-id-tests-20261025.log`. The actual unpermitted-round pipeline control passed 1/1, 8.52 s: `/tmp/misaka-rfc-body-owner-unpermitted-round-tests-20261025.log`.

Node library, tests and private-node metadata passed: `/tmp/misaka-rfc-native-owners-node-check-retry5-20261025.log`, 33.69 s. This confirms compilation, not execution of the node consumer controls. Fee-statistics runtime controls passed 3/3 (0.00 s), `/tmp/misaka-rfc-body-owner-fee-stats-tests-20261025.log`. Its 6m49s build includes lock/dependency time. Existing actual accounting-seat controls passed 5/5 (3.41 s), `/tmp/misaka-rfc-native-body-accounting-seat-tests-20261025.log`, with the retained body API in their reader closures.

Intermediate build errors remain in the preceding retry logs. They identified the body API's test-only self alias, the old owning unpermitted-round branch, the slice validator signature, and accounting test closures still returning raw Arcs. The mergeset path now borrows its coinbase view, the validator accepts a slice, and the test closures use the retained body type. GDN/router/GC fixture compile corrections in the shared build are recorded in their own evidence. No failing or interrupted attempt is counted as a passing suite.

## Still outside this change

The public RPC template render, protobuf/serialization buffers, and transaction query results create separate owning copies and need their own checked destination admission and lifetime. This change does not cover every transaction, header, EVM payload, arbitrary deep clone, historical header consumer, model cost boundary, or all payout/court/aux consumers. No public fence, preset, protocol value, or activation membership changes. Full activation remains unarmed; K3 and the 15 real campaign gates remain unexecuted.

Implementation source commit: `596d166dc` (native body/GDN/prospective routing consumers). This identifies the implemented source scope, not completion of RFC-0006 or a campaign gate.
