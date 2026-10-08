# 20261009-glm-edge-1.5b-r1 — zai-org/glm-edge-1.5b-chat@7b201d3c160c

* model: `zai-org/glm-edge-1.5b-chat` revision `7b201d3c160c25beda4cf0d107617ad975cd1ca8`, family glm, task text-generation, context 512
* tested integration SHA: `d4bf0a56f58e56e87e641cd20f3b6fe3d58236e7` (branch `onboard/h1-hf-closed-loop`, dirty=False)
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
* artifact: 1642067712 B, sha256 `bfce330d1ff26de3…`, pack `98e9cd7b62be1899…`, cache key `1a21e232e6543ba7` (clean-source run: False)
* pack verify (strict, rebuild): exit 0, verified True, failed [], skipped []
* beacon conformance: SYNTHETIC_BEACON_CONFORMANCE_PASS (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `f908d955255900ba…`, root `5455de9eb9421dda…`, owner `f768798deb6e59acec3b…`, carrier `16d8e647b4df9136…`, U spent 476650 sompi
* consensus state: all agree True, checks {'u_proves_registration_at_pin': True, 'restart_B_agrees': True, 'fresh_Z_ibd_agrees': True, 'modified_artifact_refused': {'pack_verify_exit': 2, 'registration_gate_exit': 34, 'refused': True}}
* reorg (registration mined on an isolated minority branch, then the majority): not run for this model

## Blockers / failures (failures.json)
* none recorded
