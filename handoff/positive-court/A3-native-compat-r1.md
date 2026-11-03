# A3 — A0 native contract 42a60b840 compatibility

A0's `42a60b840` was incorporated locally as `11ce122dd`. A3's result/witness DTOs, signature domains, inner G14 framing and production adapters required **no changes**. The checked r1 messages/tir/wire source files are unchanged by the native/funding addition.

The supplemental A3 test commit is **`4003824fe`**. Integrate it after the original A3 implementation `b699358de` and A0 native contract `42a60b840`; do not cherry-pick the local common dependency `11ce122dd` again into A0. Original implementation evidence `84d95e372` remains historical and unchanged.

The new test `acyclic_funding_consent_precedes_final_result_and_actual_panel_node_plan` independently derives the A1 relation count before a final Result exists; constructs basis → actual payer consent signature → Service → final Miner Result signature; and then runs PanelBuilder → original-author signed frames → actual NodeChecker to Matched on that same candidate. It refuses a stale pre-consent PlanId and invalid coherent funding terms. The fixed resolver inputs and refund payload are explicitly synthetic test metadata. Neither `AuthenticatedFunding` nor this test establishes a real UTXO, payer solvency, lock, payment, native admission or Gate C authority.

On source candidate `4003824fe`, **83 selected tests passed, zero failed**:

| Suite | Passed |
|---|---:|
| Common contract, funding, native namespace and TIR interchange | 24 |
| A1 checker/hardening/port plus A3 builder | 29 |
| Signed Panel transport/funding interoperability and G14 source pipeline | 11 |
| Source metadata guard | 1 |
| A2 responder regression | 14 |
| Default-off old G14 pursuit | 4 |

[Raw logs, source-tree identity, hashes and binary pins](A3-r1-evidence/native-42a60b840/validation.json) are retained separately from the prior logs. Documentation no-new-regressions passes with 43 known issues and zero new occurrences. The existing `scripts/ci/a3-panel-r1-ci.sh` runs all suites, including the newly added test.

Native namespace values 176/151 remain reservations. A3 treats `ChainScope.branch` as the admitted ancestor anchor: A4 must validate that anchor and its Result/Case rows in the current lineage before invoking the role APIs. A3's local scope comparisons do not qualify a moving tip, prove native admission, or restore a revoked case. Native funding/qualification, actual fees/capital, canonical state/UTXO atomicity and A5/A6 consumers remain their owners' responsibilities. Public activation is unchanged.
