# FP Job V4 — what to update, by when (operator one-pager)

**Flag day:** DAA **1,500** (≈ 2026-09-28 13:50 JST). The release that arms it must run on every node
by DAA **~1,400** (≈ 09:50 JST).

| who | update | by |
|---|---|---|
| every node (`kaspad`) | the release binary (`upgrade`, never `switch`) | DAA ~1,400 |
| FP producers | `misaka-palw-base0` worker **and** `misaka-palw-gateway`, together | DAA ~1,400 |
| FP producers | stop submitting V3 FP jobs from DAA ~1,490 (a V3 commitment landing at ≥ 1,500 is refused) | DAA 1,490 |
| seat operators | nothing beyond the node binary | — |
| API clients | optional: `repeat_penalty`, `frequency_penalty`, `presence_penalty`, `repeat_last_n`, `logit_bias`, `stop`, `temperature`, `seed` work from DAA 1,500 | after 1,500 |

Checks after upgrading (before 1,500): the node's consensus schedule id names `palw_fp_decode_rules`
at 1,500; the gateway still sends V3 jobs (`misaka.sampling.reason` says the fence is not armed).
After 1,500: the gateway sends V4 jobs; FP claims license; no `PalwFpJobVersionAtHeight` refusals in
the node log except for stale V3 submissions.

If something goes wrong: see `rollback-emergency.md`.
