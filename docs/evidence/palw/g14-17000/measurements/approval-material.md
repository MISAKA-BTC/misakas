# Decision material, not adopted policy (2026-10-26)

## Beacon source count/depth and selected-verifier threshold

The shipped `approved_panel_beacon_policies_v1()` is empty. `PostCommitChallengePolicyV1` separately carries `work_count_k` and `settlement_depth_d`; choosing them requires observed eligible-source arrivals, independence by operator/consumer, abort/withholding, reorg and latency distributions. M-SHADOW has not run, so no `k` or `D` is supported by this record.

For a **conditional** selected-verifier calculation, assume `f=0.25` means the *maximum malicious fraction*, each seat is sampled independently with equal weight, and a malicious approval requires at least `m` of `k` malicious seats. Then `P(false approval) = Σ(i=m..k) C(k,i) f^i(1-f)^(k-i)`. The illustrative majority thresholds below show `1-P(false approval)`; they are **not measured detection rates** and are invalid for correlated operators, weighted selections, withholding or missing replies. They also do not establish correct-claim liveness.

| k | m | Conditional rejection probability |
|---:|---:|---:|
| 7 | 4 | 92.9443% |
| 9 | 5 | 95.1073% |
| 17 | 9 | 98.7615% |
| 19 | 10 | 99.1097% |

If the instruction instead uses `f=0.25` as the *minimum honest fraction*, even the probability of drawing one honest seat is only `1-(1-0.25)^k` (90% at k≥9, 99% at k≥17 under independence), and a majority approval cannot be inferred from that premise. The independent reviewer needs the exact definition of `f`, the eligible population and operator-weight distribution, the selected-verifier approval semantics, and the paired attack results. No threshold is selected here.

## Three prerequisite fence policies

`palw_bond_budget_v1`, `palw_model_bond_allocation_v1`, and `palw_material_income_v3` remain without a measured candidate in this evidence. M-P15 admission curves, M-COST actual-model cost/retention, and a bounded attacker-gain versus recoverable-bond accounting are missing. Giving a numerical price or budget now would turn a fixture or an assumption into a policy. Lane VAL can attach candidate alternatives once those inputs exist; the user decides the policy.

## Independent verification-method review packet

The review must cover sampled deterministic re-execution and local court evidence, the selected-verifier approval and honest-fraction assumption, the four M-SHADOW attacks plus honest twins, source independence and withholding/grinding, and the measured compute/transfer/dispute cost for each case. The paired data and reviewer sign-off are absent; no independent-review gate is PASS. No cryptographic correctness claim is proposed.
