# A3 — typed capture prototype verification

**62 outer-suite tests passed, zero failures**, on source `94e18252f965d593e71b24f528cda05ccb29ac5e` (tree `755a71444f17034c05946ac75b7af6f556ecba94`), based on A0 `f6c7d9f4d0ddb189c70aa0084a9563d039369252`. Branch: `codex/a3-typed-capture-prototype`. Clippy completed with `-D warnings`; documentation no-new-regressions passed with 43 known issues and zero new occurrences. Historical 129/148 or other-role results are not added to this total.

| Required test scope | Passed |
|---|---:|
| typed-capture | 8 |
| runner-r1-regression | 19 |
| executor-unit | 32 |
| record-r1-regression | 3 |

The executor log contains two child-process summaries repeating test identities already represented in its outer 32-test summary. The preliminary sum of every printed summary was 64; authoritative metadata counts only the outer summaries, hence **62**, while preserving all raw output. Seven actual test binaries are SHA256-pinned. The initial mutable preflight fixture borrow error and later uncounted preflights are retained separately; all frozen-candidate checks succeeded.

[Source design and A1 API/mapping proposal](A3-typed-capture-prototype.md) explains the one-pass observer, literal original sparse program, typed records, source-only boundaries and unassigned Panel commitment. [Exact commands, hashes and raw-log metadata](A3-r1-evidence/typed-capture-f6c7d9f4d/validation.json) pins source/tree, each command/exit, files, unedited logs, binary hashes and review scope.

## Small original sparse fixture

Prefill 2/decode 3, four executed positions, repeated layer block, global/per-layer Fixed and Hist, uncommitted i64 and i128 values scaled by 2^40 and 2^80. The typed native values of every node match the independent reference evaluator; state and outputs match reference execution. Ordinary and observed execution preserve the exact old leaves, trace/execution/output roots and generated IDs. No commit flags, model program/layout/dtypes/maxctx or Miner signatures are changed.

| Hist window | Records | Nodes | Lanes | Native payload B | Input-auth scratch B | Additional peak backing B | Logical storage B | Reserved visits |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 174 | 147 | 696 | 4464 | 565712 | 598032 | 17388 | 584 |
| 3 | 174 | 147 | 756 | 4704 | 565712 | 598272 | 17628 | 605 |

Both cases include 12 after-step Fixed records, 12 appended history rows and three consumed output records. `window=1` rows survive immediate history eviction. Logical backing includes both geometry/capture metadata owners, typed payload and conservative format-ceiling authentication/fresh-runner metadata; it is **not a measured RSS bound**. Logical storage is a defined cost model; this increment writes no spool and measures no filesystem/IO cost. Existing program/params, executor state/workspace, old run retention and caller-owned copies need separate reservations.

The eight prototype tests include reference differential/old-root equality, local copy omission/order/coordinate/type/output/origin attacks, eight below-bound capture dimensions plus early authentication scratch refusal, invalid context/Flat prompt/token checks, first-callback failure, late StateWrite rollback and suppression of inherited custom selector/drill settings. Local-copy equality establishes data identity only; it does not authenticate a remote source or certify computation.

`PendingTypedRecordCommitment` remains a distinct **unassigned** type. No new public digest/domain/ID, codec, signature or native carrier exists; no old PC2R1REC or execution-root meaning is reused. A0/A1 still need to assign/authenticate the extra Panel-only commitment and bind fullrelation source access to independently derived relations and checked frontiers. Adopted wide final logits are a separate explicit profile, refused by this checkpoint before old i32 conversion; wide intermediates remain literal. The original checkout's wide WIP was not imported.

These are CPU small-fixture source checks and selected regressions. They are not real-LLM measurements, completed fullrelation acceptance, body-176/native admission, protected disk installation, real funding/locks/fees, Gate C or activation results.
