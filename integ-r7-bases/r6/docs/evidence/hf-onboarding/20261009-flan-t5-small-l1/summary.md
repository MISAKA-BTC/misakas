# 20261009-flan-t5-small-l1 — google/flan-t5-small@0fc9ddf78a1e

* model: `google/flan-t5-small` revision `0fc9ddf78a1e988dac52e2dac162b0ede4fd74ab`, family t5-encoder-decoder, task text2text-generation, context 512
* tested integration SHA: `3b9ee24e7ed503c17cb1d7212f1a52a47dfafd9a` (branch `onboard/h1-hf-closed-loop`, dirty=True)
* binaries: misaka `c9005b3265bdd8b0…`, palw-class `da87d655dcedaabe…`
* devnet: salted testnet-12 drill genesis `76a7e10dc287ac79…`, params `f2da0947fc0cca31…`, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}

| level | reached |
|---|---|
| L0 | PASS |
| L1 | no |
| L2 | no |
| L3 | no |
| L4 | no |
| L5 | GAP |
| L6 | GAP |

**Highest level: L0.** Next gate: L1 shape.

## Numbers
* artifact: None B, sha256 `…`, pack `…`, cache key `None` (clean-source run: None)
* pack verify (strict, rebuild): exit None, verified None, failed [], skipped []
* beacon conformance: NOT_RUN (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `…`, root `…`, owner `…`, carrier `…`, U spent None sompi
* consensus state: all agree None, checks {}
* reorg (registration mined on an isolated minority branch, then the majority): not run for this model

## Blockers / failures (failures.json)
* LAYOUT_OR_RESOURCE_REFUSED CLOSE_SIZE_OVER_CAP (preflight, owner C)
* TASK_UNSUPPORTED NOT_RUN_PIPELINE_ADMISSION (preflight, owner A (task profile) -> B)
