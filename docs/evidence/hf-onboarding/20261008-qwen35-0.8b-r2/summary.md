# 20261008-qwen35-0.8b-r2 — Qwen/Qwen3.5-0.8B@2fc06364715b

* model: `Qwen/Qwen3.5-0.8B` revision `2fc06364715b967f1860aea9cf38778875588b17`, family qwen3.5-gdn-hybrid, task text-generation, context 512
* tested integration SHA: `be5808434752b3999135540cb29fd6b4c8f907f5` (branch `onboard/h1-hf-closed-loop`, dirty=True)
* binaries: kaspad `6e9b193643b8b215…`, misaka `211e708db7fe0ebd…`, palw-class `e789d9c473c33639…`
* devnet: salted testnet-12 drill genesis `76a7e10dc287ac79…`, params `f2da0947fc0cca31…`, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}

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
* artifact: 1052511616 B, sha256 `4d8f3547aea0e266…`, pack `cbe4a7d33131ca00…`, cache key `e349609cefd4682e` (clean-source run: False)
* pack verify (strict, rebuild): exit 0, verified True, failed [], skipped []
* beacon conformance: SYNTHETIC_BEACON_CONFORMANCE_PASS (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `a9f20d1bdb45cc84…`, root `8f455fcf579cf93c…`, owner `f768798deb6e59acec3b…`, carrier `cb01e7f6e47656b9…`, U spent 705075 sompi
* consensus state: all agree True, checks {'u_proves_registration_at_pin': True, 'restart_B_agrees': True, 'fresh_Z_ibd_agrees': True, 'modified_artifact_refused': {'pack_verify_exit': 2, 'registration_gate_exit': 34, 'refused': True}}
* reorg (registration mined on an isolated minority branch, then the majority): not run for this model

## Blockers / failures (failures.json)
* none recorded
