# 20261011-qwen15-moe-a2.7b-l1 — Qwen/Qwen1.5-MoE-A2.7B-Chat@ec052fda178e

* model: `Qwen/Qwen1.5-MoE-A2.7B-Chat` revision `ec052fda178e241c7c443468d2fa1db6618996be`, family qwen2-moe, task text-generation, context 512
* tested integration SHA: `23c1974e949d8deff2d7c4703906cbeeae865544` (branch `onboard/h1-hf-closed-loop`, dirty=True)
* binaries: kaspad `af205146b098c4ae…`, misaka `00e720255f227bc0…`, palw-class `8f132c58dbf73829…`
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
