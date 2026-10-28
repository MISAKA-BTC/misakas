# 20261008-llama32-1b-r1 — unsloth/Llama-3.2-1B-Instruct@5a8abab4a5d6

* model: `unsloth/Llama-3.2-1B-Instruct` revision `5a8abab4a5d6f164389b1079fb721cfab8d7126c`, family llama (Meta lineage, Llama-3.2), task text-generation, context 512
* tested integration SHA: `ea402c1a898688712bc12e2b1d3b5e5553ef8e69` (branch `onboard/h1-hf-closed-loop`, dirty=True)
* binaries: kaspad `6e9b193643b8b215…`, misaka `211e708db7fe0ebd…`, palw-class `e0ff1c1b325bfded…`
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
* artifact: 1533096320 B, sha256 `8d63d1eedb204317…`, pack `540d1b35ebb34272…`, cache key `6b1f330612afb4f1` (clean-source run: False)
* pack verify (strict, rebuild): exit 0, verified True, failed [], skipped []
* beacon conformance: SYNTHETIC_BEACON_CONFORMANCE_PASS (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `534d21bd1ac1e1e2…`, root `b2aae7d8cadf7b08…`, owner `f768798deb6e59acec3b…`, carrier `2cd50258f78f4860…`, U spent 472150 sompi
* consensus state: all agree True, checks {'u_proves_registration_at_pin': True, 'restart_B_agrees': True, 'fresh_Z_ibd_agrees': True, 'modified_artifact_refused': {'pack_verify_exit': 2, 'registration_gate_exit': 34, 'refused': True}}
* reorg (registration mined on an isolated minority branch, then the majority): not run for this model

## Blockers / failures (failures.json)
* none recorded
