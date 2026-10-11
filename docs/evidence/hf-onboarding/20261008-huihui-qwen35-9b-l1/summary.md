# 20261008-huihui-qwen35-9b-l1 — huihui-ai/Huihui-Qwen3.5-9B-abliterated@05b9e7c9b978

* model: `huihui-ai/Huihui-Qwen3.5-9B-abliterated` revision `05b9e7c9b978ba29bdb8f50a49c30e4b91183339`, family qwen3.5-gdn-hybrid (VLM wrapper, text decoder), task text-generation, context 512
* tested integration SHA: `ea402c1a898688712bc12e2b1d3b5e5553ef8e69` (branch `onboard/h1-hf-closed-loop`, dirty=True)
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
* LAYOUT_OR_RESOURCE_REFUSED SEAT_MEMORY_SHORT (preflight, owner C)
