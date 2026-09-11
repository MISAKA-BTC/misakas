# ADR-0114 — The owner's leg is five percent, and it arrives at a height

* Status: PROPOSED 2026-09-11 at the operator's request ("追加で現在の売買の burn 5% 開発者への手数料 1% から
  burn 5% 開発者への手数料 5% にあげて　つまり売って買うと 20% は fee として持ってかれるようにして").
  **IMPLEMENTED the same day**, and **scheduled on testnet-11 at DAA 3,500** by the operator the same
  day (asked with the chain at ≈3,399: "DAA 3,500 で有効化"): `PALW_RC_MODEL_LEG_V2_FENCE_DAA`. Every
  other preset keeps `palw_model_leg_v2 = None`. testnet-11's printed fingerprint moves `ecbdbc22…` →
  `02c7282b…`; the identity does not, so the fleet rolls one host at a time and builds with and without
  the fence stay peers until 3,500 (`the_owner_leg_flag_day_keeps_every_current_node_until_3500`).
  **3,500 is a flag day: every node must run a build carrying the fence before it.**
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 4 (the split of
  every MSK leg: 5 % burned, 1 % to the class's registrant), [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
  Decision 8 (that leg is the line OWNER's, and an adopted contributor takes `contributor_permille_of_leg`
  of it — of the five percent past the fence), [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  (the window's `quoteBuy`/`quoteSell` and `constants()`), and [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  §4's worked table (a second table beside it, §2).
* Supersedes nothing. Leaves ADR-0091 (the reward's 5 % buys the pair, no leg) untouched.

## 0. The sentence this ADR is

**Past the fence every join and every leave splits its MSK leg 5 % burned, 5 % to the line's owner and
90 % to the curve (a join) or to the seller (a leave), so a round trip leaves about a fifth of the MSK
behind before the curve's own slippage; below the fence nothing changes, and because the fold writes the
split into the state root, the change arrives at a height every node crosses together.**

## 1. Decisions

1. **The schedule is a value, chosen at the move's DAA and nowhere else.**
   `PalwModelFeesV1 { burn_permille, leg_permille }` with `V1 = 50/10` (ADR-0087 D4 as shipped) and
   `V2 = 50/50` (`PALW_MODEL_OWNER_LEG_PERMILLE_V2`), and `PalwModelFeesV1::at(leg_v2_active)`. The fold,
   the EVM window, the RPC and the CLI each ask `at` — the fold with the block's
   `PalwTransitionExtrasV1::model_leg_v2_active`, the window with `PalwEvmMarketFencesV1::leg_v2_active`,
   the RPC at the node's virtual DAA — so no two readers can quote one move under two schedules. The
   pre-existing entry points (`palw_model_fee_split_v1`, `palw_model_buy_quote_v1`,
   `palw_model_sell_quote_v1`) keep their V1 meaning; the fenced paths call the `_with` variants.
2. **The fence is a bare activation, `Params::palw_model_leg_v2`, read through
   `palw_model_leg_v2_fence()`**, which folds in the market's own fence (a schedule for a market that
   does not exist is meaningless). It joins every place a model fence is spelled: the fence list
   (`palw_fences_v1`), the identity visitor and its never→None collapse, the Some-only write into
   `consensus_params_id`, the processor's copy, and the four shipped presets (`None`).
3. **The owner's leg is the whole change.** The burn stays 5 %. What the fold does with the leg is
   unchanged: `pay_model_leg` pays it to the line's owner payload, splits `contributor_permille_of_leg`
   of it to an adopted contributor, and burns it where a class has no registrant (M6). Rounding is
   unchanged in kind: each leg is floored, the remainder stays on the net leg, and
   `burn + registrant + net == gross` under both schedules.
4. **The RPC says which schedule is in force, and when it changes.** `getPalwModelMarket` version 6
   appends `burnPermille`, `legPermille` (at the virtual DAA) and `legV2ActivationDaa` (0 = not
   scheduled); gRPC fields 22–24. A version-5 peer decodes as 50/10 with nothing scheduled, which is
   what that peer's fold does. The CLI's previews quote with the schedule the node serves and print it;
   the site reads the same fields.

## 2. The numbers (pinned in `palw_model_market_v1.rs`)

ADR-0090 §4's table again, from the least seed (100,000 MSK, 500,000 positions), under V2:

| move | burn | owner | net | positions | reserve after |
|---|---|---|---|---|---|
| join 1,000 MSK | 50 | 50 | 900 | 4,459 out (V1: 4,656) | 100,900 MSK (price 0.20361584) |
| join 1,000 MSK | 50 | 50 | 900 | 4,381 out (V1: 4,570) | 101,800 MSK |
| leave all 8,840 | 89.9912 | 89.9912 | 1,619.8416 | 8,840 in | 100,000.176 MSK |

A join of 100 MSK and the leave of what it bought return **80.892738 MSK** (V1: 88.25488168): 0.9² of the
MSK less the slippage — the "20 % taken as fees" of the request. M2 holds exactly under both schedules.

## 3. Why a height and not an edit

The split is not a display value: the fold writes it into each market row (`msk_reserve`,
`burned_sompi`, `registrant_paid_sompi`, `contributor_paid_sompi`) and into `pending_payouts`, all inside
the state root. Editing the constant in place would re-fold every move of every node that syncs from
genesis under the new split, write a different root for every block since the first trade, and split the
network between nodes that re-folded and nodes that did not. An activation keeps every block below the
height judged exactly as it was (ADR-0083 path (a); "consensus changes by activation, not regenesis").

## 4. What does not change

The burn, the seed floor and its lock, the supply, the curve and its rounding, the product that never
falls, the refusals (a join that releases nothing, a leave that pays nothing), ADR-0091's buyback (no leg),
ADR-0095's memberships (a position still pays its holder nothing), and the carrier and EVM mechanics of a
move. A contributor's `contributor_permille_of_leg` is a share OF the leg, so past the fence the same
permille pays five times the MSK.

## 5. Arming it on testnet-11 — done at DAA 3,500 (2026-09-11)

1. Choose a DAA `H` far enough ahead for **every** node to be rebuilt — the fleet and the outside nodes
   seen on the explorer (09-10's DAA 2,400 fence was crossed unannounced and the outside nodes forked off
   at ≈2,241; that is the failure this step exists to avoid). The chain was advancing a few DAA an hour on
   2026-09-11, so a day's notice is tens of DAA, not hundreds.
2. In the testnet-11 builder beside `palw_model_benefits`:
   `params.palw_model_leg_v2 = Some(ForkActivation::new(H));` — the printed `consensus_params_id` moves
   (Some-only write), the fence-normalised identity does not, and builds with and without it stay peers
   with a "schedules a FUTURE fence differently" warning until `H`; past `H` a node without it forks off.
3. Re-pin `shipped_presets_have_pinned_fingerprints` for testnet-11 in the same commit, and announce `H`.

Done with `H = 3,500`: the fence joins testnet-11's schedule (`1150, 1900, 2150, 2400, 3500, 2125000`) and
its fork-id gate set, the fingerprint is re-pinned to `02c7282b7541011344eabb1ce6cbe6544987e7b6a7fc132413fbfff0a6a37cfd`,
the tests that reconstruct the 2,400 flag day's builds set the new fence to `None` (it was not theirs), and
README / testnet11-join-mining / testnet11-node-operator print the new startup lines and the notice.

## 6. A gap this ADR inherits and names

`consensus/src/processes/palw_state_v2_sync.rs` passes default extras for the model fences — its own
comment calls that "a REAL gap the moment `palw_model_lines` or `palw_model_evm` is armed". The new field
defaults to `false` there like its siblings. This ADR does not widen or close that gap; whoever closes it
resolves `model_leg_v2_active` with the others.

## 7. Tests

* `the_v2_schedule_pays_the_owner_five_percent_and_leaves_the_v1_table_alone` — §2's table, V1 through
  the new door is V1, the round trip, M2.
* `past_the_leg_fence_the_owner_is_paid_five_percent_of_every_move` — the fold: the same buy on the same
  parent pays 1 % below and 5 % past the fence, to the owner's payload; the leave's owner leg equals the
  burn.
* The fingerprint and fence-table tests run unchanged with the fence `None` everywhere (the shipped
  fingerprints do not move).
