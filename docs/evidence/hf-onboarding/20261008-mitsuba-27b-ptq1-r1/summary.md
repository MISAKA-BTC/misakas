# 20261008-mitsuba-27b-ptq1-r1 — isichan-ai/Mitsuba_and_HiMitsuba-27B-GGUF@33e63d450993

* model: `isichan-ai/Mitsuba_and_HiMitsuba-27B-GGUF` revision `33e63d450993500144243989b6d3f1bbb240e4d0`, family gemma3? (GGUF PTQ1_0 custom quant; text decoder only), task text-generation, context 512
* tested integration SHA: `ea402c1a898688712bc12e2b1d3b5e5553ef8e69` (branch `onboard/h1-hf-closed-loop`, dirty=True)
* binaries: kaspad `6e9b193643b8b215…`, misaka `211e708db7fe0ebd…`, palw-class `e0ff1c1b325bfded…`
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
* HF_ACCESS_FAILED HUB_UNREADABLE (source, owner external (source access) / A for the reader)
* FRONTEND_REQUIRED ARCH_REFUSED (preflight, owner A / COV-P1P2 (one generic 'text decoder inside a multimodal wrapper' mapping, shared with Huihui and SmolVLM))
* FRONTEND_REQUIRED QUANT_NO_DESCRIPTOR (preflight, owner A (quant-format descriptor for ggml type 143 + prism.hadamard) / B only if the rotation cannot be folded exactly)
