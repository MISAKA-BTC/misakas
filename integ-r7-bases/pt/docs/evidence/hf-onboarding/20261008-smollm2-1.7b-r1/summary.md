# 20261008-smollm2-1.7b-r1 — HuggingFaceTB/SmolLM2-1.7B-Instruct@31b70e2e869a

* model: `HuggingFaceTB/SmolLM2-1.7B-Instruct` revision `31b70e2e869a7173562077fd711b654946d38674`, family llama, task text-generation, context 512
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
* artifact: 1866690944 B, sha256 `c29ae564bc3085d8…`, pack `5c127d732e8353bc…`, cache key `676ac428b0afe1c9` (clean-source run: False)
* pack verify (strict, rebuild): exit 0, verified True, failed [], skipped []
* beacon conformance: SYNTHETIC_BEACON_CONFORMANCE_PASS (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `585bbe2e35f47861…`, root `6b3399022d27c009…`, owner `f768798deb6e59acec3b…`, carrier `6ae4725b43034df5…`, U spent 472250 sompi
* consensus state: all agree True, checks {'u_proves_registration_at_pin': True, 'restart_B_agrees': True, 'fresh_Z_ibd_agrees': True, 'modified_artifact_refused': {'pack_verify_exit': 2, 'registration_gate_exit': 34, 'refused': True}}
* reorg (registration mined on an isolated minority branch, then the majority): {'B_had_it_alone_while_isolated': True, 'A_equals_B_after': True, 'registered_once_after': True, 'registered_daa_moved': False, 'note': "FAIL: 'A_equals_B_after' holds only because the class was absent on BOTH after the rejoin — the registration folded on the minority (DAA 75) was reverted on B and never reached the majority; B stayed wedged (DominanceViolation) until its DB was wiped. The L4 checks above are from a SECOND registration (DAA 117) on the restored network. See failures.json MINORITY_WEDGED_DominanceViolation / REGISTRATION_LOST_AFTER_PARTITION.", 'converged': False}

## Blockers / failures (failures.json)
* REGISTRY_STATE_MISMATCH MINORITY_WEDGED_DominanceViolation (reorg, owner D (fork choice after a partition; node recovery deadlock) / Lead)
* REGISTRY_STATE_MISMATCH REGISTRATION_LOST_AFTER_PARTITION (observe, owner D)
