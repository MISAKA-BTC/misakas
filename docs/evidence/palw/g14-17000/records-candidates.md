# G14 17,000 measurement record candidates (2026-10-26)

Network: `testnet-12`; genesis hash: `a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23` (the repo's t12 launch record and the public wRPC pruning point agree). Proposed activation DAA: **17,000**, unarmed. Source checked out for this measurement work: `84d7186471454c050c85fd3170b1ba9dea0d9987`. No arming record, digest or policy value is pinned by this document.

| Candidate requirement | Evidence digest | Status/reason |
|---|---|---|
| `palw_claim_hard_deadline_v1`: measured five `B_*` terms and `c_retain` on slowest supported node | — | **未充足**: M-X3 has no full-route run or real retention price. |
| `palw_court_capacity_v1`: measured operation classes, block budget, `rho_court` and session weight | — | **未充足**: M-C quiet/minimum-node run and M-CAP concurrency/drills absent. |
| kernel Panel=1 route: paired claim verdict, Final time and attack costs | — | **未充足**: M-SHADOW has not run. |
| RFC-0016 / RFC-0006–0008 measured records: real class resources and DA economics | — | **未充足**: M-C missing operation classes and M-COST real artifact absent. |
| prerequisites `palw_bond_budget_v1`, `palw_model_bond_allocation_v1`, `palw_material_income_v3` | — | **未充足**: M-P15 and M-COST / economic audit absent. |
| beacon/selected-verifier policy and independent review | — | **未充足**: source independence, paired attack data, reviewer decision and measured `k/D` absent. |

The public wRPC observation under [M-OPS](measurements/M-OPS/README.md) is an operational data point, not an arming-requirement digest. Each future requirement needs its own file and SHA-256 only after it has actually been run and independently checked. Other G14 fence requirements (node E2E, restart/IBD/reorg, shipped-binary drills) are likewise unfilled here.
