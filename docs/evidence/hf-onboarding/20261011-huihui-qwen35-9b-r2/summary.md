# 20261011-huihui-qwen35-9b-r2 — huihui-ai/Huihui-Qwen3.5-9B-abliterated@05b9e7c9b978

* model: `huihui-ai/Huihui-Qwen3.5-9B-abliterated` revision `05b9e7c9b978ba29bdb8f50a49c30e4b91183339`, family qwen3.5-gdn-hybrid (VLM wrapper, text decoder), task text-generation, context 8192
* tested integration SHA: `23c1974e949d8deff2d7c4703906cbeeae865544` (branch `onboard/h1-hf-closed-loop`, dirty=True)
* binaries: kaspad `b9fb67730bca0969…`, misaka `9d3be910c8610d38…`, palw-class `bdc76da0e885858a…`
* devnet: salted testnet-12 drill genesis `7944dd26f39390e6…`, params `34ca67745368d2e2…`, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}

| level | reached |
|---|---|
| L0 | PASS |
| L1 | PASS |
| L2 | PASS |
| L3 | PASS |
| L4 | PASS |
| L5 | GAP |
| L6 | GAP |

**Highest level: L4.** Next gate: L5 (G14_INCOMPLETE + BEACON_UNAVAILABLE + DA: no chain state).

## Numbers
* artifact: 10118057600 B, sha256 `cf1ba53b3d4513ea…`, pack `43ca73cbf4f2f19f…`, cache key `9e958ff3d4e06f49` (clean-source run: False)
* pack verify (strict, rebuild): exit 0, verified True, failed [], skipped []
* beacon conformance: SYNTHETIC_BEACON_CONFORMANCE_PASS (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `8e90e27fea9e22e0…`, root `96415a29ca74f93c…`, owner `73bfd1314dfb52fcc493…`, carrier `14b26942fb102281…`, U spent 709525 sompi
* consensus state: all agree True, checks {'u_proves_registration_at_pin': True, 'restart_B_agrees': True, 'fresh_Z_ibd_agrees': True, 'modified_artifact_refused': {'pack_verify_exit': 2, 'registration_gate_exit': 34, 'refused': True}}
* reorg (registration mined on an isolated minority branch, then the majority): not run for this model

## Blockers / failures (failures.json)
* LAYOUT_OR_RESOURCE_REFUSED SEAT_MEMORY_SHORT (preflight, owner C)
