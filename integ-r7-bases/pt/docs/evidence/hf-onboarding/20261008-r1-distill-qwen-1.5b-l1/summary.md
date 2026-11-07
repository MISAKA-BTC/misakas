# 20261008-r1-distill-qwen-1.5b-l1 — deepseek-ai/DeepSeek-R1-Distill-Qwen-1.5B@ad9f0ae0864d

* model: `deepseek-ai/DeepSeek-R1-Distill-Qwen-1.5B` revision `ad9f0ae0864d7fbcd1cd905e3c6c5b069cc8b562`, family qwen2-dense (DeepSeek-R1 distill: a Qwen backbone), task text-generation, context 512
* tested integration SHA: `89ffb1fb717166ccc9810464605939f95d8acc2f` (branch `onboard/h1-hf-closed-loop`, dirty=False)
* binaries: kaspad `6e9b193643b8b215…`, misaka `211e708db7fe0ebd…`, palw-class `e0ff1c1b325bfded…`
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
