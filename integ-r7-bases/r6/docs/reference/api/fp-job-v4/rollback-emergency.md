# FP Job V4 — rollback and emergency procedure

RFC-0001 §A.7/§A.8: release and activation are separate, and V3 is never re-activated once V4 is.

## Before the fence (DAA < 1,500)

The release changes no consensus rule before the fence (the fence is the only arming; isolation and
the header-context door refuse V4 below it). A node can go back to the previous release binary:

1. `kit upgrade-rollback` on the node (in place, one node at a time; the unit is preserved). Pass the
   previous release's fingerprint as `UPGRADE_FROM_FP`.
2. Roll the gateway and worker back together (the V3 worker refuses V4 requests; the old gateway
   sends V3 only).
3. If the whole fleet must stay on the old rules, the coordinator ships a release whose flag-day list
   omits `PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1` **before** DAA 1,500 — the height moves; nothing
   unverified ships.

## After the fence (DAA ≥ 1,500)

Do not roll back to a pre-V4 binary: it refuses every V4 block past the fence and forks off.

| symptom | action |
|---|---|
| honest V4 claims do not license (seats file nothing) | collect the seat log lines naming the claim; a node patch (engine or replay) ships as an emergency node update — no consensus change |
| a V4 claim that should be refused licenses (seat replay gap) | the court's arithmetic closes still apply; the fix is a node patch; if the rule itself is wrong, a V5 emergency fence (new job version, new fence) — never a return to V3 |
| gateways refuse or mis-normalize requests | roll back the gateway only (the chain rule is unaffected); gateways below the fence send V3, above it V4 |
| stop strings do not stop | expected when the model tokenizes the string differently; not a fault |

## Evidence to keep

The claim id, the job (`misaka.fp_job_id`), the answer ids, the node and worker versions, and the
seat verdict lines. `decode_answer_stop_v4(config, limit, vocab, answer)` and the golden vectors
(`consensus-vectors/fp-v4/`) reproduce any selection offline.
