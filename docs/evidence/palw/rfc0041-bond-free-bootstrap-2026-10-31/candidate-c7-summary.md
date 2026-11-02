# RFC-0041 — bond-free bootstrap integration evidence (2026-10-31)

**Candidate C7: PASS.** The seven-package EVM-enabled libtest build succeeded. RFC-0041's three core tests and one exact private-node integration test passed, with no ignored or zero-match result. The node test took **1315.115 seconds**. The source inventory, build artifacts, commands, executed binary SHA256 and log SHA256 are bound in [verification.json](verification.json).

This is the implementation/finite private-node evidence for [RFC-0041](../../../rfc/0041-genesis-bond-free-bootstrap.md) / [ADR-0209](../../../adr/0209-genesis-bond-free-bootstrap.md). It does not announce public deployment. The user explicitly allowed other sessions to continue editing and requested separate results for each candidate: **C7 is not a PASS for subsequent shared edits**. Build9 later failed in an unrelated, still-being-integrated Prefix court method; its failure and earlier diagnostics remain recorded separately.

## Actual path and funding

The node starts with no `BondRegistered` in its genesis registry. A recovery at genesis timestamp + 1,800,000 ms is accepted, while the preceding millisecond, a forged signature and a losing class draw are refused. A child coinbase actually pays the recovery worker subsidy. Recovery creates no bond, seat, or certified claim; ordinary heartbeats remain fee-only.

The eight registrations **do not use one BASE reward as all their collateral**. They use the owned `MAIN_PREMINE_INDEX` genesis UTXO, a normally signed ordinary spend on the winning reorg branch, and successive real change outputs. Each bond locks 93,906,321,001,040 sompi through its normal transaction, registration signature and operator-possession signature. The new private genesis UTXO commitment is recomputed; no existing network history is changed.

The reorg includes a genuinely competing signed spend. The selected branch changes, the orphaned bond and its collateral output disappear, and registration continues from the winner's spendable change. Refused recovery candidates are also replayed through normal validation because they can remain DAG parents. A nonconflicting transaction is not assumed to disappear merely because the selected chain changes.

## Authentication, clocks and restart

- The unchanged **1000 DAA** bond maturity and **5040 DAA** approved verifier maturity advance through actual validated heartbeat blocks. No state is planted and no maturity/window is shortened.
- The first ordinary PALW attempt wins its real class lottery. Its actual selected Panel excludes the producer. A corrupted receipt quorum is refused; genuine ML-DSA-87 signatures on a funded carrier license the claim.
- `Final` alone is asserted not certified. A later execution anchor draws the global pool under the unchanged **75/41** policy. All 75 drawn positions approve through the selected ordinary bonds; the producer is excluded. Positions are drawn with replacement, so this is not a claim of 75 distinct operators.
- The **5040 DAA** prosecution window remains in force. Only afterwards does `SecurityCertified` become true. Consensus is stopped and reopened over the same DB without genesis import; its state root and UTXO set match, and a further normal heartbeat succeeds.

The approval premise remains explicit: sufficient honest drawn stake, participation and access are assumed. Signatures authenticate approvals; they do not establish arithmetic truth. This harness uses fixture execution/trace/transcript roots, not a real LLM replay campaign. It skips network PoW as the inherited harness does while retaining the class lottery, signatures and normal maturation. EVM is compiled in the unified build but inert in this harness. Gate C/kernel fences are armed only in the isolated test configuration after base validation; this is not a public arming record. No model review, model-coefficient change or deferred image/video/improvement activation is introduced.

## Reproduction and records

The exact node test is:

```text
pipeline::virtual_processor::tests::t12_round_lane_e2e::bond_free_bootstrap::rfc0041_production_recovery_pays_then_public_bonds_start_palw
```

The dedicated source is [rfc0041_bond_free_bootstrap.rs](../../../../consensus/src/pipeline/virtual_processor/tests/rfc0041_bond_free_bootstrap.rs). The [source inventory](candidate-c7-source.json) identifies C7 despite the dirty shared checkout; a HEAD alone would not do so. The [build log](candidate-c7-build.jsonl), [core listing](candidate-c7-core-list.txt), [node listing](candidate-c7-node-list.txt), [core log](candidate-c7-core.log), and [node log](candidate-c7-node.log) retain the actual results. Commands, features, toolchain and test-binary hashes are in [verification.json](verification.json). Raw runner records conservatively label direct executions diagnostic; the candidate-specific verification record binds those runs to the successful C7 build and makes no statement about later candidates. Cached executable paths may be overwritten by later Cargo builds; the recorded SHA256 identifies what actually ran.
