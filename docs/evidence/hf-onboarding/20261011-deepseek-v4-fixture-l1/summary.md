# 20261011-deepseek-v4-fixture-l1 — None@

* model: `None` revision `None`, family deepseek_v4 (in-repo synthetic fixture), task text-generation, context 512
* tested integration SHA: `23c1974e949d8deff2d7c4703906cbeeae865544` (branch `onboard/h1-hf-closed-loop`, dirty=True)
* binaries: kaspad `af205146b098c4ae…`, misaka `00e720255f227bc0…`, palw-class `8f132c58dbf73829…`
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
* FRONTEND_REQUIRED QUANT_NO_DESCRIPTOR (preflight, owner A)
* LAYOUT_OR_RESOURCE_REFUSED CLOSE_SIZE_OVER_CAP (preflight, owner C)
* LAYOUT_OR_RESOURCE_REFUSED COURT_COST_OVER_CEILING (preflight, owner C)
