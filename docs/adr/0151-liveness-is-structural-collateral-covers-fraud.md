# ADR-0151 — Liveness is structural; collateral covers fraud

* Status: **D2, D3, D4 and D5 in force on testnet-12 from genesis; D1's genesis half landed, its
  runtime half open; D6 partially.** D4 and D5 were drafted as OPEN from a description of the defect
  and turned out to be already built — that correction is recorded in their sections rather than
  edited away.
  testnet-12 is the last testnet before mainnet, so the operator's decision (2026-09-22) was to start
  it on the separated design rather than ship the capital buffer and remove it later.
* Date: 2026-09-22
* Retires, on testnet-12: the `window_bind × dearest claim` genesis collateral requirement in
  `palw_genesis_v2` (`BondCannotSustainBindWindow`). It stays in force wherever the structural
  guarantee is not proved.
* Related: ADR-0061, ADR-0066 (the beat is outside `bits`), ADR-0071, ADR-0133 §7/§11.3,
  ADR-0137 (a share is a result), ADR-0138 §3b (the stand-in tick), ADR-0142 (the clock cursor),
  ADR-0144 §9 (the lock ledger), ADR-0145 (work is derived).

## 1. The defect, measured

testnet-12 registers the held Qwen2.5 row at `n_ctx` 2,097,152 — the class the fleet already mines.
Sizing its genesis bonds produced four figures, and the distance between them is the subject:

| figure | what produced it | per seat |
|---|---|---|
| 38,889,673.34 MSK | `palw_v2_collateral_for_claim_lifetime_v1`: `max(per-claim) × (MAX_CLAIM_EXPOSURE_DAA + 1)` = ×7,201 | the derivation as shipped |
| 1,620,178.03 MSK | `verify_palw_genesis_v2`: `dearest per-claim × window_bind` = ×600 | the genesis liveness bound |
| **21,630.05 MSK** | reachable concurrency: floor at 7,201 claims + each model row at its in-flight cap ×4 | **what testnet-12 ships** |
| 5,400.59 MSK | ONE 2M claim's exposure | the unit all of them are built from |

The first is a straightforward over-derivation, caught by the operator: it applies the FLOOR's
concurrency — claims arrive one a block, nothing caps them — to a class the class-local admission gate
caps at one in flight. 7,201× where 4× would do.

The second is not a pricing error, which is why it survived the correction. It defends a cycle:

```
collateral exhausted -> no claim admitted -> no block produced -> DAA frozen
  -> no BindTimeout -> collateral never released
```

DAA advances only when blocks are produced, so a bond that cannot hold `window_bind` claims at once
fills its ceiling and the chain stops — no timeout, no operator action, no message. The bound buys the
way out with capital, and on a test network funded from the operator's premine that is cheap.

**It is the wrong instrument, and mainnet is where that bites.** It makes the initial operators the
only parties who can afford a seat, and it makes a genesis seat and a later seat obey different
economics for the same duty — `1.62M because you are genesis, 2,700 because you arrived later` is
workable operationally and indefensible as a protocol. So it is cut where it closes: at the clock.

## 2. Decision

**Liveness is guaranteed by protocol structure. Collateral covers fraud liability and nothing else.**

### D2 — the genesis-only collateral rule is conditional, then gone — **ARMED**

`verify_palw_genesis_v2_with_clock_v1` takes one fact — can this chain advance its clock without
admitting a claim? — and skips the bind-window requirement when it holds. The old entry point
delegates with `false`, so **the conservative behaviour is the default** and only a caller holding
`Params` can claim otherwise. Nothing else in the gate is relaxed: C-08 (a bond declares only what its
outpoint holds), the panel-seating requirement and the catalog agreement all still run.

### D3 — the clock does not depend on any bond's collateral — **ARMED**

`palw_clock_advances_without_a_claim_v1(&Params)` is two clauses:

* **the heartbeat lane is armed from genesis.** A beat carries no claim and reserves no exposure, so a
  bond whose ceiling is completely full can still mint one;
* **no lane `bits` prices is producible.** On a `ConsensusV2` network every V1 proof-of-work activation
  is `never()`, and `algo_id_is_priced_by_bits_v3` takes the attempt, execution, receipt and round
  lanes out of the pricing.

**The beat is not a priced lane either, and that is the premise rather than a gap.** ADR-0066 took the
heartbeat out of `bits` deliberately, so `palw_lane_advances_daa_v1` answers *no* for it — the beat's
contribution is decided per MERGESET, not per lane. Because the second clause makes `priced == 0` on
every mergeset, ADR-0138 §3b's stand-in fires on every mergeset carrying a beat
(`stand_in = priced == 0 && heartbeats > 0`, in `DifficultyManagerExtension::daa_exempt_count`) and
exactly one beat is counted into the score in the missing anchor's place — at most once per wall-clock
interval under ADR-0142's cursor.

So the DAA advances every heartbeat interval regardless of what any bond can afford. Both clauses rest
on activations that are `never()` and a fence armed at 0, so the answer cannot become false later.

**The operational consequence, stated because it is a real one:** at one tick per 120 s the 600-DAA
bind window is ~20 hours of wall clock and the 7,200-DAA exposure span is ~10 days. Bounded, not
collateral-dependent — but a claim's exposure is held far longer in wall-clock terms than the figures
suggest, and D4 is what stops that from blocking new work.

### D1 — collateral is reachable fraud liability — **GENESIS HALF LANDED**

`palw_v2_collateral_for_class_set_v1` makes two corrections, which pull in opposite directions:

* **concurrency, per class instead of the dearest** — the floor at `MAX_CLAIM_EXPOSURE_DAA + 1`
  (genuinely reachable), each model row at `PALW_MODEL_CLAIM_CONCURRENCY_V1 = 4` (its enforced cap ×4);
* **liability, per claim, is `palw_max_fraud_gain_v1` = `escrowed_reward + fork_weight(pwu, slash)`** —
  the fraud a Valid Final AUTHORIZES, not the compute it counts.

**The second correction RAISES the figure, and that was the finding.** The expectation was a refinement
downward; measured, the dense row's gain is 2,702.96 MSK against 1,350.15 MSK of `pwu × slash`, because
a claim's `pwu` is the expected attempt count times one inference — so the fork weight a Final buys is
twice what the bond reserves against it. And the ESCROW half of a gain does not shrink with a cheap
claim, so the floor's term (7,201 concurrent × 2.66814 MSK) becomes the largest of the three.

| class | concurrent | gain/claim | collateral |
|---|---|---|---|
| BASE-0 floor | 7,201 | 2.66814 MSK | 38,426.56 MSK |
| held Qwen3.6 @512 | 4 | 4.73917 MSK | 37.91 MSK |
| held Qwen2.5 @2M | 4 | 2,702.96409 MSK | 21,623.71 MSK |
| | | | **60,088.18 MSK** |

Eight seats: 480,705.47 MSK, 0.0048 % of the 10B cap.

**The runtime half is NOT done, and this is the one place testnet-12 knowingly ships a gap.** A
producer's live reservation is still `palw_exposure_pwu_v1 × slash_value`, i.e. half the gain on the
dense row. Closing it means splitting a value the design keeps unified on purpose: that same
`DerivedV1` arm feeds ADR-0124's work price and ADR-0125's execution credit, and "paid on the same
number it can be slashed on" is what `work_priced_escrow` rests on. Its own commit, on the running
network.

### D4 — admission capacity and slash liability are separate ledgers — **ALREADY BUILT, armed here**

Drafted as OPEN on the premise that "one ledger does both jobs". **That premise is false**, and the
check is recorded rather than the draft quietly fixed:

* `reserved_exposure` is the CAPACITY ledger. `release_for_claim` runs **on `Final` and on `Voided`
  alike** — "the exposure and the immature contribution both belong only to non-terminal claims" — so
  processing capacity returns the moment a claim resolves.
* `slashable_locks: BTreeMap<(bond, claim), PalwSlashableLockV1>` is the LIABILITY ledger:
  `{ claim, amount, expiry_daa }`, `is_live(now_daa)`, no claim bytes retained. Its own doc is this
  decision's sentence: *"Final does not erase liability … withdraw is refused while any lock on the
  bond is live."*

What testnet-12 adds is that the second is armed from DAA 0 (`palw_objective_offence`, which
testnet-11 schedules at 8,500). Pinned by `t12_adr0151_d4_d5::d4_the_liability_ledger_is_armed_from_genesis`.

### D5 — one derived profile per class, and duration is not weight — **ALREADY BUILT, armed here**

Also drafted as OPEN, also already true:

* **duration is not weight.** `palw_economic_payout_v1` contains no `window`, no `_daa`, no
  `deadline` and no `expiry` — the reward is `min(escrow, C_P × rate)` and the panel's share is
  `clamp(α·C_V / (C_P + α·C_V), S_min, S_max)`. A source-level test pins the absence, because the
  property is "no such term exists" and only the text can state that.
* **the deadline is the class's own.** ADR-0133 §11.3's `max(window_receipt,
  verification_window_spans × span_daa)`, armed from genesis here (`palw_class_receipt_window`), which
  is what lets the 2M row leave `Probation` without weakening the network's deadline for anybody.

### D6 — the gate asks for progress, not for capital — **HALF DONE**

The question has changed: `verify_palw_genesis_v2_with_clock_v1` asks a FACT about the network's clock
rather than sizing capital. What it does not yet do is run §4's reachable-state search against the
card. That search is owed as a drill before deployment (§4) and belongs in the gate before mainnet.

## 3. What this does NOT change

* **ADR-0137 stands.** A share is a result; block rights are `verified work / execution quantum` behind
  the execution lane's one permit a round. This ADR is about collateral, not cadence.
* **Fraud stays unprofitable.** D1 lowers collateral toward the honest liability and never below it:
  `collateral ≥ max_fraud_gain / minimum_colluding_quorum` is a floor, not a target.
* **No other network moves.** testnet-11 satisfies D3's clauses too, so its gate result is unchanged;
  its declared collateral was never gate-derived. `shipped_presets_have_pinned_fingerprints` and
  `every_genesis_commits_to_the_premine_this_build_mints` confirm every other preset is byte-identical.

## 4. The evidence still owed — a DEPLOYMENT blocker for testnet-12

The collateral was lowered on a rule-level proof: `consensus/tests/palw_t12_liveness.rs` shows
`priced == 0` at every height and that the predicate the gate reads answers yes. **That is the premise,
not the behaviour.** The stand-in's firing needs a store, and the property is about reachable states.

Before testnet-12 carries value, a wedge search must show the timeout clock advances from every one of:

* the 2M class locked to its cap and the hybrid locked to its cap, simultaneously;
* one panel seat dropped;
* immediately after a restart;
* immediately after a reorg;
* during IBD;
* one DAA before a receipt deadline;
* several classes at their caps at once.

For each: the chain produces a block, the DAA advances, a timeout fires, liability is released. A green
predicate is not a wedge search, and this ADR does not claim it is.

## 5. Order of the remaining work

1. **testnet-12 genesis** — D2, D3, D4, D5 and D1's genesis half. Done 2026-09-22. **§4's wedge search
   before the network carries value.**
2. **On the running t12** — D1's runtime half: the producer's live reservation raised to the gain its
   claim authorizes, which means separating the reservation from the work price and the execution
   credit. t12 is the network that rehearses it.
3. **Before mainnet** — D6 in full (the gate runs §4's search), and the removal of the
   `window_bind × dearest` code path entirely rather than its conditioning. A mainnet card must not be
   populated while that path can still be reached.
