# RFC-0041 — candidate C10 (2026-10-31)

**PASS:** seven-package EVM-enabled integration build; RFC0041 core **3/3**, exact private-node **1/1**, related Prefix regressions **11/11**. No ignored or zero-match result. Node elapsed: **1487.277 seconds**.

The earlier missing `close_conviction_with_subject_audit_v1` integration failure is resolved in this build. The normal audit path is retained, while old FP source convictions cannot borrow a later raw-ID cohort's auditor receipts. Related Prefix source identity, old financial basis, expiry/re-admission, responsibility and retention regressions all passed. The correction was already present in the shared source; this follow-up verified it rather than replacing it with a weaker path.

The complete RFC0041 route again passed: empty genesis bond registry, exact 30-minute header-time boundary and atomic refusal, actual BASE coinbase, owned genesis UTXO through normal signed spending and registration, 1000 DAA bond maturity, 5040 DAA verifier maturity, actual PALW/selected Panel, authentic receipts, Final without credit, approved 75/41 selected-verifier authentication, 5040 DAA prosecution window, and same-DB Consensus restart/continuation. Eight bonds are funded by the owned genesis UTXO and actual successive change, not one BASE reward. Execution roots remain fixtures, network PoW is skipped as in the inherited private harness, the class lottery/signatures/maturity are enforced, and honest-stake/participation/access assumptions remain explicit. This is not public deployment or an LLM replay campaign.

## Identity and concurrent edits

The exact executables were copied out of Cargo's mutable cache, SHA-checked, and made read-only before running. [execution-manifest.json](execution-manifest.json) records their preserved locations and SHA256. [verification.json](verification.json) binds those executable bytes to the successful build, commands, features, exact nonzero listings and logs.

Other sessions continued editing as requested. The [before-build](source-before.json), [after-build](source-after-build.json), and [after-test](source-after-tests.json) inventories retain those changes separately. C10 PASS identifies the tested executable candidate; it does not promote future source changes or claim a frozen whole-tree release.

## Records

- [Build](build1.jsonl)
- [RFC0041 core log](core.log) and [listing](core.list.txt)
- [Exact node log](node.log) and [listing](node.list.txt)
- [Related Prefix regression log](prefix-regression.log) and [listing](prefix-regression.list.txt)
- [Earlier C7 record](../verification.json), preserved without changes
