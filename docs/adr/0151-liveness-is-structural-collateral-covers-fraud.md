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

## 3a. The live evidence, taken 2026-09-23

Run with the binary being shipped (`de857a71`, a release build of this branch), two nodes on a fresh
testnet-12 genesis and no class artifact — the point being that the clock moves without one.

```
[palw-heartbeat-miner] starting — bondless heartbeat lane (ADR-0060), fee-only, one thread
[palw-heartbeat-miner] heartbeat #1 8b557c1e… — the clock ticked
[dns-bft] anchor=a8cabac47b96fe30…          ← the pinned t12 genesis
Consensus rule manifest: … palw_work_target=1 palw_independence=1
PALW court certified end-to-end for: PALW-BASE-0, PALW-QWEN36, PALW-QWEN25-A16, PALW-QWEN25-A16-V5
```

Four heartbeats in the first two minutes, eight blocks relayed to the peer, zero errors. **"Bondless"
is D3 in the node's own words**: the lane that carries this chain's clock takes no bond, so no
exposure ceiling and no collateral figure can stop it. That is the premise the collateral reduction
rests on, now observed rather than argued.

Restart and partition-rejoin were clean (0 errors, the surviving node kept minting, the returning one
rejoined). **One drill defect worth recording rather than hiding:** the first pass used `--connect`
for the peer links and measured an IBD node that never synced. `--connect` puts a node in
outbound-only mode and it does not listen for inbound P2P at all — there is no "P2P Server starting"
line — so the drill had silently built a topology with no listener. The live fleet already encodes
this distinction (`--addpeer` on the two hosts that accept inbound, `--connect` on the local-only
seats) and the switch plan must preserve it exactly.

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

## Addendum 2026-09-23 — what the first testnet-12 fleet and the item 6 acceptance run found

Two of this ADR's numbers were posted in units no runtime path reads, and the acceptance ladder that
followed found three memory paths that would have killed a 2M seat before it produced a claim. All
are fixed on `feat/testnet-12-regenesis`; the genesis block is unchanged and only the params
fingerprint moved (`fb8f378d…` → `c746f07c…` → `30848c6b…` → `bcfbf2a3…`, the last for `palw_unavailable_abstains`, a t11-genesis fence the t12 preset had left dormant; `t12_arms_every_fence_t11_armed` now holds every t11-armed fence to genesis on t12).

### The two defects of the first deployment

1. **Collateral in the wrong unit.** §D1's carve priced each class on its own declared leaves;
   the runtime reserves `palw_exposure_pwu_v3` — the derived MAC-eq draw renormalised by the
   floor's leaves-per-MAC-eq — 44.25× larger on the 2M row. Every producer held at `produced=0`
   from genesis. Collateral 60,088.18 → 516,429.80 MSK a seat (`2bd134ec`). The raw fork weight of
   one 2M Final is 335,728,175.72 MSK, 3.36 % of the supply, so "collateral covers the fraud gain"
   is a statement about the EXPOSURE unit; the weight unit is D1's remaining half.
2. **A flat `artifact_digest` pinned where the operand-inventory root belonged** — the same
   defect testnet-11's card records — over a byte-identical artifact. Every seat: `holds no artifact
   whose registered root form is b5baca63…`. Closed structurally, not by pinning a third value:
   distinct types with no conversion (`palw_class_identity_v1`), a `.palwmanifest` beside the
   artifact written by the same derivation the runtime resolves with, the manifest committed and
   parsed by a `const fn` so the genesis card carries no hand-written hash, and
   `--palw-verify-class-manifest` refusing to start on a disagreement (`ea7ad7df`, `26030bc1`,
   `077d4c7f`, `2f588a58`).

### The three memory paths the acceptance ladder found (item 6)

With the streamed root (`216641a4`) already in place, a lone 2M seat on an isolated chain grew from
5.9 to 23.7 GiB of anonymous memory on nothing but heartbeats and the panel's per-tick pre-checks,
and was OOM-killed twice without producing. Named and fixed:

* `RopeTableV1::digest_bytes` built a 1.07 GiB `Vec` of the rotary table per `artifact_digest()`
  call, and every class resolve, cache key and manifest check calls that, from three threads at
  once. The digest now streams (`digest_into`), byte-identical, so no class id moves (`e40dfcd2`).
* The panel's replay-budget pre-check resolved the class — compiling its plan and hashing the
  artifact — on every tick. Memoized on (holdings, class, root) (`e40dfcd2`).
* The readiness proof materialized the whole inventory (retained twice), copied every leaf hash,
  and re-folded the tree per drawn leaf, and an unsubmittable proof was rebuilt every tick. The
  material is now one streamed walk and the built proof is kept per (class, span) (`427a1b8d`).

### What the ladder also taught about the ladder

A node cannot mine alone (`peers=false` holds regardless of `--enable-unsynced-mining`), so a
one-node rung measures idle and the manifest verify only; blocks appear at the rung where a peer
exists. An isolated chain's first block needs `--enable-unsynced-mining` (in the fleet only `ibm`
carries it). A verification flag that finds no manifest must refuse, not report "0, all agreeing".
Leftover nodes from a previous run hold their ports and ignore `SIGINT`; a ladder starts by killing
them. The three fixed memory phases bracket construction only; a periodic line and an external
sampler are what bracket growth during operation.

### The fourth path, and the gate, measured (item 6, continued)

With the three above fixed, the producer sat flat at 2.3 GiB of anonymous memory until a peer
arrived and blocks flowed — then rose 16.05 GiB in ONE minute and was killed. Not a leak: the K/V
cache of one attempt. `A16Cache` is `Vec<Vec<Vec<i32>>>` twice, and the 2M row's canonical job
prefills 262,143 positions: 28 × 2 × 256 × 4 B = 56 KiB a position, 14.55 GiB with headers. The
producer's path had epoch and exposure gates and no memory gate; the panel's replay estimate was the
artifact's file size plus 512 MiB (3.17 GiB), five times too small.

`a16_attempt_working_set_bytes_v1` names the cost once, beside the cache; the producer asks
`attempt_working_set_bytes(prefill)` right before it spawns and HOLDS; the panel's replay figure
carries the K/V term (`82575797`). Verified on 5.104.81.23 (23 GiB, two nodes, budget 21 GiB / 4):

```
this attempt would allocate 17.23 GiB for a 262143-token prefill (its K/V cache plus scratch)
and the host's replay budget is 13.74 GiB … — holding rather than being OOM-killed
producer alive; 10 heartbeat blocks; no kill; steady anon 3.85 GiB
```

One transient remains unexplained: anon 1.08 → 9.79 GiB about a minute after start, released within
two minutes, with the attempt already refused. It is a working-set term no figure names yet
(candidates: the plan compile on the first resolve, the readiness material walk) and is assigned to
the engine track below.

**This gate is stage one, not the answer.** On 24 GiB hosts the 2M row is now a *named hold*
rather than a crash, which also means it does not produce there. The operator's direction for the
engine (tracked on `feat/kv-codec`): the runtime representation is not `CanonicalWork` — i32/i16/i8,
paged/resident, mmap/RAM, CPU/GPU may all change while `execution_root`, verdict, `CanonicalWork`,
payout and quanta stay byte-identical. Order: working-set canonicalization (producer / full seat /
partial seat / checkpoint / KV-resident / scratch, derived from one canonical profile and separated
from file size) → producer/panel/Court through one derivation → a node-local memory reservation
ledger (`estimate → atomic reserve → execute → release`, because a correct per-attempt figure still
OOMs when three seats each see "available") → S1 partial seats resuming only their segment → i16
then i8 KV as versioned runtime profiles that never change a class's economic identity → RPC
telemetry (`estimatedWorkingSet`, `reservedMemory`, `holdReason`, …) → cross-runtime determinism
tests. Memory requirement is capacity; collateral is `max_fraud_gain`; the two are separate systems.

Post-fix ladder figures (PSS per seat at 1 / 2 / 4 nodes) are appended when that run completes.
