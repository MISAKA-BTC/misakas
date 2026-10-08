# 20261008-qwen25-0.5b-r1 — Qwen/Qwen2.5-0.5B-Instruct@7ae557604adf

* model: `Qwen/Qwen2.5-0.5B-Instruct` revision `7ae557604adf67be50417f59c2c2f167def9a775`, family qwen2-dense, task text-generation, context 512
* tested integration SHA: `89ffb1fb717166ccc9810464605939f95d8acc2f` (branch `onboard/h1-hf-closed-loop`, dirty=False)
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
* artifact: 660666048 B, sha256 `9af80e4e459cc233…`, pack `72c046533ff1ba34…`, cache key `f03cec68202dc546` (clean-source run: False)
* pack verify (strict, rebuild): exit 0, verified True, failed [], skipped []
* beacon conformance: SYNTHETIC_BEACON_CONFORMANCE_PASS (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `55b32ae12c8fa003…`, root `3c458f9988b7ae33…`, owner `f768798deb6e59acec3b…`, carrier `de0998b5632ff7f6…`, U spent 475212 sompi
* consensus state: all agree True, checks {'u_proves_registration_at_pin': True, 'restart_B_agrees': True, 'fresh_Z_ibd_agrees': True, 'modified_artifact_refused': {'pack_verify_exit': 2, 'registration_gate_exit': 34, 'refused': True}}

## Blockers / failures (failures.json)
* REGISTRATION_QUOTE_INVALID E-MODEL-UNKNOWN (no one-command HF/IR registration) (register, owner A (HF source->artifact in model add) / C1 (quote, detached sign/submit for ClassRegisteredTirV1) / Lead (UX contract))
* SIGNATURE_OR_RELAY_FAILED NODELESS_BOND_REGISTRATION_ABSENT (register, owner C1 (remote wallet: node-less bond registration))
