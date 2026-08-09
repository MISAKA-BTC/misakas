# MISAKA VLT — step 4 recovery proof (2026-08-09)

Not a code change: the record that the fixes in `d7f9f78`, `4f8794e`, `1a78966`, `524ea7a` and
`91a9fd7` make verified compute actually credit **on a running chain**, through all three stages —
live walk, accumulator persistence, and restart.

One `5 validator(s) with credit` line was explicitly *not* accepted as the pass condition. The
failure being fixed produced exactly that line before losing everything a minute later.

## Setup

Five-validator devnet, `--shadow-only` (weight fence dormant), one fixture job each
(`--job-quotas 1,1,1,1,1`). Work dir `.misaka-vlt-devnet-2`.

`plan_id = <runtime_hash>:1:15:41` — target 1, `max_tokens` 15, prompt 41 bytes.

## The eight checks

| # | check | result |
|---|-------|--------|
| 1 | five logical slots, each certified exactly once | **PASS** — every node `"Certified"` on one job id |
| 2 | confirming verdicts per certificate | **PASS** — 20 total, 4 per certificate against a threshold of 3 |
| 3 | refuted = 0 | **PASS** |
| 4 | challenge window matured | **PASS** — `challenge_not_mature` cleared on its own |
| 5 | accumulator row non-empty | **PASS** — credit survives the finalization boundary |
| 6 | snapshot root identical on five nodes | **PASS** — `d7a6dc7d679aa65d…` on all five |
| 7 | same row after restart | **PASS** — node-4 restarted 21:39:28, agrees at 21:41:30 |
| 8 | no permanent skip reason on the five | **PASS** — only `(not yet)` reasons ever appeared |

## The decisive evidence (check 5)

Same chain, same certificates, same verdicts. The only change between the two runs is where the
verifier-committee pool comes from.

```
before 91a9fd7      after 91a9fd7
sink 1899  credit 5     sink 2199  credit 5   W(E)=221,323,000
sink 1999  credit 0     sink 2299  credit 5   W(E)=214,683,250
sink 2099  credit 0
```

Before, credit appeared for exactly one epoch and then vanished for good: the walk floor rose past
a capability declaration made at DAA 1348, the candidate pool emptied, and every honest verdict
belonged to no committee. After, the credit persists and *decays* as §4 specifies rather than
dropping to zero:

```
250,000,000 µRTE  = 5 jobs x normalize_vlt(fixture job)   (raw)
221,323,000       = 250,000,000 x 0.97^4                  (epoch 20)
214,683,250       = 250,000,000 x 0.97^5                  (epoch 21)
```

The expectation is derived, not hardcoded: `normalize_vlt` prices one fixture job at 50 VLT from the
registered profile's rho and the preset's (a, b), and `decay_coefficient` supplies the rest.

Check 5 is what the earlier failure would have failed. At sink 2199 epoch 15 is finalized
(2199 − 1500 = 699 > challenge window + reorg horizon = 600), so its row is served from the
accumulator and is *not* re-derived by the walk. Non-zero credit at that point is therefore proof
that the persisted row is non-empty.

## Capability backfill

All five nodes reported `swept 5 capability declaration(s) out of history into the capability
store` once, and once only, on the first commit after upgrading — the marker prevented a second
sweep across node-4's restart. Without it the store would have started empty on a chain whose
declarations were already accepted, and an empty pool is indistinguishable from "nobody declared".

## Job identity

Five distinct jobs, one per validator; each node's certificate names the same job id as its own
commitment, so the 6/5 shape that `1a78966` fixed does not recur.

```
node-0  7392e5136f88b7cd…    node-3  f4b3b6085ef2e647…
node-1  104f84fb24c5c3f5…    node-4  2328ee12041909b1…
node-2  bee234c483f9f842…
```

## What this chain may and may not be used for

Usable for the C activation smoke test: five credited validators at ~250,000,000 µRTE raw against
`min_network_compute = 200,000,000` clears the floor with one job to spare.

**Not** usable for the 8/5/3/2/2 experiment. These five equal credits stay inside the credit window
and would add to any new plan's weight; a new `plan_id` renames the quota, it does not erase
certificates already on chain. That needs its own clean chain.
