# G14 17,000 measurement record candidates (2026-10-26)

Network: `testnet-12`; genesis hash: `a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23` (the repo's t12 launch record and the public wRPC pruning point agree). Proposed activation DAA: **17,000**, unarmed. Source checked out for this measurement work: `84d7186471454c050c85fd3170b1ba9dea0d9987`. No arming record, digest or policy value is pinned by this document.

| Candidate requirement | Evidence digest | Status/reason |
|---|---|---|
| `palw_claim_hard_deadline_v1`: measured five `B_*` terms and `c_retain` on slowest supported node | — | **未充足**: M-X3 has no full-route run or real retention price. |
| `palw_court_capacity_v1`: measured operation classes, block budget, `rho_court` and session weight | — | **未充足**: M-C quiet/minimum-node run and M-CAP concurrency/drills absent. |
| kernel Panel=1 route: claim-level selected-verifier approval and paired verdict, Final time and attack costs | — | **未充足**: A limited kernel Panel=3/1 same-claim comparison completed 60 processes; a separate Panel=1 Gate C measurement completed 120 processes. Both are loaded fixtures. Legacy-route comparison, randomness withholding, minimum-node measurements and approved draw-cap policy remain missing. |
| RFC-0016 / RFC-0006–0008 measured records: real class resources and DA economics | — | **未充足**: M-C missing operation classes; Dolphin artifact is now verified but runtime/court/DA measurements are pending; 9B absent. |
| prerequisites `palw_bond_budget_v1`, `palw_model_bond_allocation_v1`, `palw_material_income_v3` | — | **未充足**: M-P15 and M-COST / economic audit absent. |
| beacon/selected-verifier policy and independent review | — | **未充足**: source independence, paired attack data, reviewer decision and measured `k/D` absent. |

For the two measured G14 fences, the exact requirement order in `palw_g14_gates_v1.rs` remains entirely blank:

| Fence / index | Requirement from the source | Evidence SHA-256 | Reason |
|---|---|---|---|
| X3 / 1 | V1 build, regressions, boundary decode | — | No suite run for this source. |
| X3 / 2 | Measured five `B_*` terms and `c_retain` | — | M-X3 NOT_MET, including minimum node and retention price. |
| X3 / 3 | Node E2E of fixed deadline and non-extension | — | No node E2E run. |
| X3 / 4 | Shipped-binary fence/restart/reorg drill | — | No drill run. |
| Court capacity / 1 | V1 build, regressions, boundary decode | — | No complete suite run for this source. |
| Court capacity / 2 | Measured every operation, bytes, working set and block allotment | — | M-C incomplete; minimum node absent. |
| Court capacity / 3 | Measured `rho_court`, session weight and occupancy economics | — | M-CAP NOT_MET. |
| Court capacity / 4 | Full-slot coalition, independent verifier meets deadline | — | No saturation drill. |
| Court capacity / 5 | Continuously refilling coalition, queued prosecution first | — | No refill drill. |
| Court capacity / 6 | Maximum-load block validation within resource bounds | — | No maximum-load drill. |
| Court capacity / 7 | Restart/IBD/pruned/reorg mid-queue | — | No replay drill. |

The public wRPC observation under [M-OPS](measurements/M-OPS/README.md) is an operational data point, not an arming-requirement digest. Each future requirement needs its own file and SHA-256 only after it has actually been run and independently checked. Other G14 fence requirements remain unfilled here.

Continuation source review: [parameter audit](measurements/approval-material.md) inspects `2119c943b`, separately from the older source of the fixed court binary. It finds a claim/position cap mismatch and an unsupported inclusion-bound derivation. Its arithmetic checks and public observations do not fill any arming digest above. The current kernel slice includes claim hard deadline; M-X3 cannot be omitted without an explicit slice change and dependency review.
