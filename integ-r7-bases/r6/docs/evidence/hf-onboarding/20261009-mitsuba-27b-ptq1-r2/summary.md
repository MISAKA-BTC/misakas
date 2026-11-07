# 20261009-mitsuba-27b-ptq1-r2 — isichan-ai/Mitsuba_and_HiMitsuba-27B-GGUF@33e63d450993

* model: `isichan-ai/Mitsuba_and_HiMitsuba-27B-GGUF` revision `33e63d450993500144243989b6d3f1bbb240e4d0`, family gemma3? (GGUF PTQ1_0 custom quant; text decoder only), task text-generation, context 512
* tested integration SHA: `b7d7a4bc9c081a90d77ef6d9aba5e8c71b14e589` (branch `onboard/h1-hf-closed-loop`, dirty=True)
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
* HF_ACCESS_FAILED HUB_UNREADABLE (source, owner external (source access) / A for the reader)
* LAYOUT_OR_RESOURCE_REFUSED SEAT_MEMORY_SHORT (preflight, owner C)
* TEST_INFRASTRUCTURE_FAILED ARTIFACT_DISK_GAP (artifact, owner H1)
