# A7 entitlement adapter: pending draft, not applied

Status: **UNAPPLIED DRAFT / NOT TYPECHECKED OR EXECUTED / EIGHT-USE API NOT READY**.
The patch is preparation only. It does not change the current A7 crate or revise any historical test result. Replace it with a final patch after the complete A0/A5 candidate and API-ready notice; the eight-use draft is based on observed uncommitted API and must not be applied before that API is committed, accepted and rechecked.

- Patch: `a7-a5-entitlement-update.pending.patch`.
- Exact base: A7 `c8bfb180843917d36530d02b4532bfa4a1d81113`; the two source files have the same bytes as the earlier `bd9623914` candidate.
- Required seven-use port: A5 `dd8efe1ab1034555e30b828d9471b7e2503e1dcd`, adding `RootedCreditView::authorize_entitlement` and `TirCreditView` forwarding. This dependency has not been merged by this task.
- Additional uncommitted API observation: A5 worktree `/Users/wata/.codex/worktrees/positive-court-a5/MISAKA-testnet` (HEAD still `dd8efe1ab`) contains appended `CreditUse::WorkerPayment` in `positive-contract/src/ports.rs` and `CreditConsumers::worker_payment` in `positive-accounting/src/credit/consumers.rs`. The draft now names these observed APIs and covers eight uses. This is NOT a ready/acceptance notification: exact committed candidate, unchanged signatures/semantics and readiness must be rechecked before finalizing or applying the patch.
- Ownership: only proposed changes to A7 `tests/pipeline.rs` and `tests/pipeline_support/mod.rs`. No production changes, merge, Cargo run, or target creation.

## Two independent existing-rights fixture oracles

The common `PrivateEconomicFixture<E>` contains explicitly synthetic Positive/Gate C/fraud/maturity/funding prerequisites. Those prerequisites never generate a use-specific entitlement. An immutable oracle is attached separately and compares the complete `CreditBinding` (scope, claim, case, result, output and dependency commitment) for the selected use.

The existing-primary oracle records seven individually named existing rights and binds each to that exact binding. These are declared test facts representing an existing primary origin/contract, not a native registration adapter and not a policy that infers All from PositivePassed. The separate `PaidWorkerRightsFixture` declares only distinct pre-existing WorkerPayment and Settlement rights and refuses the other six uses. The primary oracle explicitly refuses WorkerPayment. Neither oracle derives rights from fee-ledger state, readiness, PositivePassed or P.

## Draft eight-use expectations

These are literal test expectations, independent of the fixture oracle implementation. Every allowed use also requires the actual A1 checked terminal and all shared Positive prerequisites.

| Use | Existing primary fixture | Paid worker fixture | Missing entitlement port |
| --- | --- | --- | --- |
| Reward | allow | deny | deny |
| WorkReceipt | allow | deny | deny |
| DagWeight | allow | deny | deny |
| RuleE | allow | deny | deny |
| Settlement | allow | allow | deny |
| PrefixReuse | allow | deny | deny |
| Dependency | allow | deny | deny |
| WorkerPayment (observed uncommitted API) | deny | allow | deny |

The draft preserves the existing real bound A2 inference/signature → real A3 builder → A1 CheckedTerminal → TirCreditView path. Existing tests explicitly select the primary fixture: its seven old consumers retain literal expectations while WorkerPayment is always refused. Native-unassigned WorkerPayment is also refused. Two proposed tests use the real pipeline to verify paid WorkerPayment and Settlement success with the other six uses refused, and to verify the default missing port refuses all eight uses despite all other prerequisites. Paid success checks each returned authorization's literal `usage()` and exact `binding()`, catching crossed WorkerPayment/Settlement routing even though both are allowed. Each binding component is mutated and both paid rights must reject it. A literal ordered eight-use inventory is compared against exported `ALL_CREDIT_USES`; negative loops use the literal inventory rather than deriving expectations from implementation policy.

## Separate payment conditions

Source-provider fees for already supplied service and refunds follow their own existing service/refund conditions; they are not successful-worker payment and must not be routed through a new Positive/P success prerequisite. No fee-ledger readiness, funded balance, local Positive success or P value creates Reward, WorkReceipt, DagWeight, RuleE or other rights. The pending WorkerPayment consumer is exclusively for its pre-existing successful-worker contract entitlement.

## Checks and limits

Rustfmt with the repository `.rustfmt.toml` and edition 2024 parsed the temporary proposed files; `git apply --check` checks the patch against the unchanged A7 source. No Rust typecheck or tests have been run for this draft. Recorded historical evidence continues to describe its original candidate/API and must not be cited as passing this entitlement update. After API readiness: recheck this draft against the accepted eight-use API/commit, replace it with the final patch, apply only to the accepted candidate, then let the parent coordinate the full verification run.

A separate read-only reviewer checked the two oracles, literal expectations, purpose/binding assertions, closed default and payment separation. No additional concrete issue or obvious type error was found in that static review. The paid binding mutation test covers scope by changing its branch; separate network/ruleset mutations are not included. The oracle compares the whole scope through full binding equality. This review is not a Rust typecheck or execution result.

Observed API source SHA-256 (read-only working-tree snapshot, not readiness evidence):

- `misaka/palw/positive-contract/src/ports.rs`: `3a91a53bb431c031ca8f45db8bc2d5650f0688025186822dcb0cdae854f96475`.
- `misaka/palw/positive-accounting/src/credit/consumers.rs`: `6a509566121166e67e63c5e58ef853ed0cc4cff6362555cc52845a680b3d0289`.
- `misaka/palw/positive-accounting/src/credit.rs`: `0f874c66521a83903ac2fe717902e1849c3478622184fa62393294528510b633`.
