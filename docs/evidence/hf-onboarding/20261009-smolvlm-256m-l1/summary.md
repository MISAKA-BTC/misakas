# 20261009-smolvlm-256m-l1 — HuggingFaceTB/SmolVLM-256M-Instruct@7e3e67edbbed

* model: `HuggingFaceTB/SmolVLM-256M-Instruct` revision `7e3e67edbbed1bf9888184d9df282b700a323964`, family idefics3-vlm, task image-text-to-text, context 512
* tested integration SHA: `3b9ee24e7ed503c17cb1d7212f1a52a47dfafd9a` (branch `onboard/h1-hf-closed-loop`, dirty=True)
* binaries: misaka `c9005b3265bdd8b0…`, palw-class `da87d655dcedaabe…`
* devnet: salted testnet-12 drill genesis `76a7e10dc287ac79…`, params `f2da0947fc0cca31…`, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}

| level | reached |
|---|---|
| L0 | PASS |
| L1 | PASS |
| L2 | no |
| L3 | no |
| L4 | no |
| L5 | GAP |
| L6 | GAP |

**Highest level: L1.** Next gate: L2 artifact.

## Numbers
* artifact: None B, sha256 `…`, pack `…`, cache key `None` (clean-source run: None)
* pack verify (strict, rebuild): exit None, verified None, failed [], skipped []
* beacon conformance: NOT_RUN (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `…`, root `…`, owner `…`, carrier `…`, U spent None sompi
* consensus state: all agree None, checks {}
* reorg (registration mined on an isolated minority branch, then the majority): not run for this model

## Blockers / failures (failures.json)
* none recorded
