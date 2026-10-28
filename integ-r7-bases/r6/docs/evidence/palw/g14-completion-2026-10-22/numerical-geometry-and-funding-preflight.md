# Numerical geometry and transaction-bound funding preflight — 2026-10-22

Status: **isolated implementation; selected verification passed; G14 incomplete**. This extends the [native fee/source checkpoint](native-fee-and-source-graph.md) without enabling any preset, new price or public funded-original path.

## Numerical topology

`consensus/core/src/palw_court_numerical_geometry_v3.rs` resolves the opaque native source against the registered layout, verifies its binding and analyzes ranges once for its scan. It visits every original leaf, finds each native dissection site and recursively enumerates all native child ranges through each bottom tile. It requires no future producer root value or accepting DAA. Paths are immutable private fields; no caller-selected subset constructor or deserializer exists.

Before allocating each root's paths, the native analytical tree inventory bounds paths, edges, retained path bytes and depth. The iterator independently checks its resulting counts and native cuts. Explicit original-scan/root/path/edge/path-byte budgets refuse the entire result when insufficient. These are enumeration limits, not calibrated CPU, heap or retention costs. Source decoding and verification still need native resource admission.

Composition derives the source graph and numerical topology from the same original source. Its coverage is **SourceAndNumericalGeometryOnly**: court/demand remain explicit parameters, and composition does not prove that they equal the accepting network's pinned rules. It does not emit role-specific numerical service packets, authenticated root values, court results, semantic/end-effect branches or an admitted original. Parameter cover still has its separate equivalent prelude; shared analysis across both components is unfinished.

## Exact funding preflight

`consensus/core/src/palw_court_funding_preflight_v1.rs` reuses the existing signed funding preparation and checked carrier fee budget. Its private result binds the streamed full transaction serialization (including signature scripts, sigop counts and mass excluded from raw txid), every populated input entry, parent state root, selected parent block, accepting point, expected funding scope/network and explicit fee obligations.

The same actual input-minus-output fee is allocated once across fixed obligation purposes. Escrow principal is already an actual output and is not added to fee purchasing power. Rechecking refuses another transaction/entry/root/point/scope/profile/horizon/requirement. Hash binding authenticates the identity of the supplied material; it does not make caller-populated entries an accepted UTXO view or caller requirements a complete native policy.

The tests use an actual MLDSA funding-consent signature and synthetic populated entries. They do not execute native input scripts, create an escrow lock, record a fee receipt or install a rooted financial row. This preflight is not an input-authorization token; ordinary validation cannot be skipped with it. The [atomic adapter conditions](../../../rfc/0035-court-original-funded-contract-and-role-consent.md#91-同branchの事前plancommit復旧の接続条件) still require same-branch native derivation, qualified spending and a matching final rooted delta.

## Verification

[Commands, source reconstruction, source hashes and pinned executable metadata](numerical-preflight-verification.json) preserve this selection. The first geometry run [failed two assertions](numerical-preflight-numerical-geometry.log): its tiny fixture had no numerical reduction sites, so it could not exercise the required nonempty path set or path-byte budget. The original source and executable remain preserved. The correction uses the existing golden history-GQA fixture and independently asserts actual single-tile, multi-tile and ragged-history sites; no implementation, cap or assertion was weakened. The [separate repair run](numerical-preflight-repair-verification.json) records the new build and verifies that the non-test geometry source prefix is unchanged. Its numerical all-arity and budget tests passed, but [full-history composition refused the fixed source query budget](numerical-preflight-repair-numerical-geometry.log). The next selection retains those nonempty numerical tests and separates the original small, empty-numerical composition success from the larger history's explicit refusal. It does not increase any enumeration budget or claim successful nonempty full-history composition. [Final selected rebuild](numerical-preflight-final-verification.json) records that source; the final selection completed successfully. [Other initial selections](numerical-preflight-initial-other-tests.json), including the three funding preflight tests and t12 identity, passed on the initial pinned executable. Repeated selections will be counted once. No real-model latency, saturation, actual price, Panel profit or heartbeat measurement is claimed. The separately scoped [fixture verifier measurement](fixture-verifier-measurement.md) does not establish those quantities.

## Original audit assets restored

The original consistency-audit ZIP supplied two missing raw JSON files for RFC-0026. Their byte lengths and SHA-256 match the existing MANIFEST exactly; all previously imported asset hashes are unchanged. [Restoration provenance](audit-assets-restoration.json) and [document check](docs-after-audit-assets-restoration.log) are retained. Missing references decreased from 48 to 46: 43 baseline occurrences plus three imported draft-source links. No baseline or historical log was changed, and the document check still fails.

## Remaining original closure

A complete coordinator must derive the accepting native rules, all numerical and semantic duties and their role consent/dependencies, parameters/prompt/material acquisition, calibrated service/fees/capital/retention, real escrow locking and qualified settlement. Actual accepting-block service, F02 default eligibility/no-slash recovery, the lone-outsider F03 campaign, D2 C1 native opening, qualified financial pruning and T01–T15 remain incomplete. No quota, typed preflight or geometry upper grants these permissions.


## Completed selected checks

The final release selection passed **39 unique tests**: numerical geometry 3 (all native arities under H7 demand, explicit scan/root/path/edge/path-byte budgets, empty-numerical composition and fixed-budget history refusal), transaction-bound funding 3, source graph 4, carrier fee 2, funding preparation 4, funded envelope 4, legacy envelope 5, t12 identity 9 and activation 5. Repeated earlier selections are not added. [Parsed counts and post-generator source checks](numerical-preflight-selected-summary.json) preserve the accounting.

Core and consensus test typechecks passed, release test executables were copied and hashed, and source hashes remained unchanged through execution. The non-test geometry prefix is identical across the fixture repairs. The explicit history query count is **130,699**, above the unchanged test enumeration budget of 100,000; this is a fixture geometry count, not a measured capacity or a new protocol limit. Nonempty full-history composition success remains unverified.

[Native wire generation](numerical-preflight-final-native-wire.log) is idempotent (exit 0). [Document check](numerical-preflight-final-docs-check.log) remains FAIL at 46 missing-reference occurrences (43 baseline plus three imported draft-source links), after restoring two exact original assets. No new authored missing link or baseline rewrite is used. The separately recorded five-operation fixture timing run has 1,000 expected outcomes per operation and separate setup baselines; those repetitions are not 5,000 additional consensus tests.

## Later native address correction — 2026-10-22

A real native input-validator regression found that the initial candidate conflated the unkeyed payer identity with the domain-separated native P2PKH address. Its helper successes therefore did not establish actual fallback spendability. The [preserved failure and corrective verification](material-and-native-candidate.md) separate those identities and check the actual public-key-derived output/refund address. Earlier source snapshots, executables, command outputs and measurement values remain unchanged; this note does not upgrade their scope.
