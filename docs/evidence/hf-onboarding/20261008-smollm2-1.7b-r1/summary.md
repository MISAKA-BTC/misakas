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
| L3 | no |
| L4 | no |
| L5 | GAP |
| L6 | GAP |

**Highest level: L2.** Next gate: L3 registration.

## Numbers
* artifact: 1866690944 B, sha256 `c29ae564bc3085d8…`, pack `5c127d732e8353bc…`, cache key `676ac428b0afe1c9` (clean-source run: False)
* pack verify (strict, rebuild): exit 0, verified True, failed [], skipped []
* beacon conformance: SYNTHETIC_BEACON_CONFORMANCE_PASS (facts SYNTHETIC; policy UNAPPROVED)
* registration: class `…`, root `…`, owner `…`, carrier `…`, U spent None sompi
* consensus state: all agree None, checks {}
* reorg (registration mined on an isolated minority branch, then the majority): not run for this model

## Blockers / failures (failures.json)
* none recorded
