# Lane KNODE report — kernel route Panel=1 roles on the node (2026-10-26)

Branch `claude/g14-knode` = `d8aded6de` + lane TNODE2's driver (cherry-picked) + this lane. **Node-side only**: no consensus rule, param, wire format, or DB meaning changes, and no fence moves (so the fleet identity below 17,000 is unchanged by construction; `t12-fleet-identity.sh` was not run).

## Correction to TNODE2's gap list
Two roles TNODE2 listed as missing **already exist** in kaspad: `palw_panel/palw_kernel_verifier.rs` (lane G14-K) implements the **outsider filer** (own replay from public commitments → seal → `FileProof`/demand via `pursuit`) and the **kernel interim seat** (Panel=1 seat receipt after its own replay). Both are armed by identity plus `--palw-kernel-model` and need no new flag.

## Added: `kaspad/src/palw_kernel_roles.rs` (child of `PalwPanelService`, runs in the tick right after the verifier, uses the same court queue and carriers)
| role | flag | what it does |
|---|---|---|
| poster | `--palw-kernel-post-job=<tok,..>:<n>` | posts one greedy job of the class the held model opens to |
| producer | `--palw-kernel-producer` [`--palw-kernel-export-dir=<dir>`] | greedy decode with its own model → evidence → `SealClaim` → reveal (salted past the salted-seal height; the salt is recomputed from a deterministic signature). Answers every open `FileDemand` on its claims with `Respond`; the export writes `.palwresp` files |
| Gate C drawn verifier | none (identity + model) | for each `'K'` row (aux 136) that is drawn, undecided, before its deadline and names this bond: replays it and signs `AttestEligibilityExamV1` (Reproduced / NotReproducible / Unavailable). This is the drawn verifiers' approval under the honest-fraction premise; it makes no cryptographic proof of computation |
| relay | `--palw-kernel-relay-dir=<dir>` | signs and carries the `.palwresp` answers for open, unanswered demands (T08) |
| drill injection | `--palw-drill-kernel-inject=lie\|silent\|forged-receipt\|wrong-challenge\|junk-flood`, `--palw-drill-kernel-flood=<n>` | **TEST ONLY.** `palw_drill_validate_args_v1` refuses them without `--palw-drill-genesis-salt`, which also covers public testnet-12 and shipped presets |

## Tests (targeted; 5 passed, 80.8 s)
- `palw_kernel_roles::tests::{injections_and_post_job_parse_and_refuse_garbage, relay_files_round_trip_and_bad_files_are_reported}`
- `palw_drill::tests::the_kernel_injections_are_refused_off_a_salted_chain`
- Actual-service e2e (`palw_service_kernel_roles_e2e.rs`, over G14-K's harness: onboarding-registered `wide128_v1`, Panel-licensed route):
  - `knode_poster_and_producer_services_carry_an_honest_claim_answer_its_demand_and_a_wrong_challenge_is_dismissed`: the poster service posts the job; the producer service commits the claim and answers a demand (and exports the answer); the Panel covers the claim; the injector service seals and files a wrong challenge; the claim is not convicted.
  - `knode_a_silent_lying_producer_and_a_relay_service_carries_its_committed_answer`: a lie+silent producer service commits a lying claim and answers nothing; the relay service answers the demand with one `Respond` it signed itself.

**Not done**: the local multi-node process run (scratch datadir) did not happen. The slice ruleset is still refused (`palw_settlement_once_v1` without `palw_bond_budget_v1`, see the TNODE report), so no slice node can start yet. The forged-receipt and junk-flood injections have only a parse/refusal test; whether the mempool or the fold drops them on real processes is unverified.

## Driver
`scripts/drills/g14_tnode_slice.py` gains `KNODE_ROLE_ARGS`/`knode_args(role, model, work)` and `--knode-role-args=<role> [--knode-model=<f>] [--work=<w>]`, which prints the kaspad flags for roles poster, producer, seat, gate-c-verifier, outsider-filer, relay, liar, silent, forger, wrong-challenger, flooder. Each node additionally needs `--palw-panel-key`, `--palw-panel-bond`, `--palw-fee-outpoint` and `--palw-kernel-model`. The per-T orchestration (which node gets which role) still has to be wired into `g14_t12_16000_fence_drill` once the ruleset builds.

## Remaining
1. CLOCK: add the bond budget to the slice (blocker), then run the T01–T05, T08, T10, T11, T13 process runs.
2. The original's atomic paid admission (F02 S1/S2) is not on the kernel route; the producer uses the existing seal/reveal admission.
3. The Panel=1 seat has no `challenge` receipt: on a fault it signs nothing and the filer files.
4. There is no separate court-signer process (T08 "distinct signer"); the relay covers carrying a third party's answer.
