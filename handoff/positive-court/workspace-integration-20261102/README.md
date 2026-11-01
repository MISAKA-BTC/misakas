# Eight-workspace integration — 2026-11-02

All eight pinned A0–A7 workspace tips are ancestors of merge commit
`ece4dc472`. Integration continues in the existing A0 checkout
`/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet`, branch
`codex/positive-court-a0`. The original main checkout has other ongoing work;
its uncommitted files were not imported or rewritten.

## Merge reconciliation

The initial A0 candidate `9fc9a9cd9` already contained the implementation and
most evidence through selected cherry-picks. Normal merges now connect the
original histories. A1's conflicting lock and registration tests retain A0's
later dependency, stack, first-writer and reorg fixes. A2's source-ledger
conflict retains the equivalent, centrally checked let-chain cleanup. Before
each A1–A5 merge commit, the staged tree was verified equal to the initial A0
tree. A6 already pointed to that exact A0 commit.

A7 contributes 21 previously missing evidence files. Each was checked against
the pinned A7 tip. At `ece4dc472`, these are the only tree changes from the
initial A0 candidate; no executable source was changed by merging. The complete
input tips, file reconciliation, conflict decisions and ancestry checks are in
[integration-inputs.json](integration-inputs.json). The retained destination
diffs for A1, A2 and A4 are stored beside it.

## Continued work and validation

Standard Python unittest discovery then exposed an actual runner failure:
`test_real_llm_process.py` parsed its required `--driver` during import. The
[original failure](python-discovery-before-fix.log) retains 26 successful tests
and one import error; it is not a passing run.

Fix `f8bb847c9` moves guard execution into `run_guards`, keeps argument parsing
under the standalone entry point, and restores the temporary driver import
path. Two discovery tests execute the existing guards and check import safety
with hostile unittest arguments and forbidden fixture execution. An AST
comparison confirms that all 18 original guard assertions are unchanged. The
standalone command retains its JSON fields, guard names and order.

At the same fixed source commit `f8bb847c9`:

| Scope | Result | Raw output |
| --- | --- | --- |
| Python controller, manifest and driver discovery | 28 passed | [Python log](final-python-discovery.log) |
| Common contract | 51 passed | [Rust log](final-contract-accounting.log) |
| Credit/accounting, including independent graph and conservation tests | 53 passed | [Rust log](final-contract-accounting.log) |
| Documentation | no new regressions; 43 existing missing references remain | [Docs log](docs-check.log) |

The separate [standalone guard invocation](driver-standalone-after-fix.log)
also passed all 18 checks. These repeat checks already exercised inside the
Python discovery suite and are not added to its test count. No model was run
by these synthetic guards.

[Final validation](final-validation.json) records commands, exit codes, test
counts, hashes and the fixed source commit. The
[source snapshot](final-source-snapshot.json) was unchanged before and after
the final checks. Rust ran offline with the existing shared target directory,
two build jobs, incremental compilation disabled and debug information disabled
for the selected leaf packages. No Rust stack override was introduced.

## Remaining implementation boundary

This merge and selected regression run do not complete production native
Assignment/Result authority, actual role and funding resolution, P approvals,
all credit consumers, or full node restart/reorg/IBD/pruning acceptance. The
native-assignment RFC/ADR drafts remain unadopted. Existing selected-source
registration and ordinary trained-model results keep their original source
commits and qualification boundaries; they are not relabelled as executions
of this merge candidate. Public activation stays disabled.

Subsequent implementation should start from this integrated A0 branch and the
current native authority/role/funding boundaries in
[the integration checkpoint](../A0-integration-status.md). The current model
tiers, claim-level verification and prohibition on cryptographic computation
proofs are unchanged.
