# Bond budget (ADR-0176) and model-bond allocation (ADR-0177) — engine design

**Lane:** BUDGET (`budget/adr176-177`, from the integration head `b8ae9412b`), 2026-10-10.
**Specification:** [ADR-0176](../../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md),
[ADR-0177](../../adr/0177-model-bond-allocation-without-availability-consensus.md),
[RFC-0015 §8.3–8.5](../../rfc/0015-panel-free-permissionless-verification.md), [RFC-0014 §16.5–16.8](../../rfc/0014-panel-independent-fraud-prosecution.md),
[PRINCIPLES](../../PRINCIPLES.md) §4/§6, [readiness matrix §3c](activation-readiness-matrix.md).
**Fences:** `palw_bond_budget_v1` (ADR-0176) and `palw_model_bond_allocation_v1` (ADR-0177 D3–D5). Both are dormant: `None` on
every preset, hashed Some-only, collapsed from `Some(never())`, listed in `palw_fences_v1()`, probed by the fork-id gate, and
refused by validation when armed. **Nothing here is armable.**

Status words follow the user's 2026-10-09 vocabulary: *implemented* (code exists), *verified* (compiles, unit + real-node E2E +
required attack tests pass on this branch), *armable* (also economic parameters, external review, activation conditions). This
note is a design; §11 records what each milestone reached.

Amounts are written in BILI (ADR-0174); `1 BILI = 10^8` base units in code (`SOMPI_PER_KASPA`).

---

## 0. What the engine is, in one paragraph

Every reward-bearing PALW claim accepted past `palw_bond_budget_v1` **reserves** at acceptance, against its producer bond, a
vector `(claims, block units, reward, Final weight)` = `(Q, B, R, F)`. A bond may hold, inside a rolling window of `W` DAA keyed
on the claims' **acceptance** DAA, at most the caps `Q_max(C, W, ρ)`, `B_max(C, W)`, `R_max(C, W)` and `F_max(C, W)`. Those caps
depend only on the bond's locked capital `C` and the policy. Every later writer **consumes** from the claim's own reservation:
the reward-block writer, the payout and maturity writers, and the Final-weight writer. A writer never pays or credits more than
the claim's remaining reservation, and the excess is never minted and never credited. A reservation leaves the window at
`reuse_not_before = d + W` and at no other time, whatever happens to the claim earlier (Final, void, conviction, retry). Past
`palw_model_bond_allocation_v1`, each claim's reward reservation is also bounded by its model's share of the realized PALW budget
`R_m = R_PALW · f(S_m) / Σ_j f(S_j)`. Here `S_m` is the distinct locked capital that bonds assign to model `m` with a signed
object. The model share only clips reward. It never refuses a claim and never changes weight, so it cannot touch block cadence,
DAA or fork choice.

## 1. Allocations and the collision check

| Item | Use | Verified unused before first use |
|---|---|---|
| Object tag **140** | `BondCapitalAssignedV1` (ADR-0177 D3, signed by the bond) | yes: no variant 140–149 on HEAD, `rfc8/x8r-review`, `g14/r4-fixes`, `opv/bootstrap-beacon`, `k2/real-scale`, `fin/palw-finality-consistency`, `g14/completion`, `fix/a2-uniform-new-tags` |
| Tags 141–149 | reserved for the lane (not used) | — |
| Delta **190** | `BondBudgetRow { table, key, old, new }`: one generic row entry for every budget table; a new table takes a table id, not a delta number | yes (HEAD highest 173; X8R uses 180–181) |
| Delta **191** | `BondBudgetHeader { old, new }` (the engine header; `None → Some` creates the engine) | yes |
| Deltas 192–199 | reserved | — |
| Carriage tail **0xEF** | the engine state, present only when `Some` | yes (HEAD uses ≤ 0xED; X8R 0xEE) |
| V2 root block **`bond_budget/v1`** | header digest + rows root, written only when `Some` | new label |
| RPC ops **250–253** | designed in §8.4 (`getPalwBondBudget`, `getPalwClaimBudget`, `getPalwModelAllocation`, `getPalwCapitalAssignment`); proto fields 1314–1321 by the 1234 + 2·(op − 210) rule | yes (HEAD highest op 231; X8R 240) |
| Fences | `palw_bond_budget_v1`, `palw_model_bond_allocation_v1` | yes (no `.rs` mentions) |

No collision was found. The integration matrix's rule ("no lane edits `PalwConsensusObjectV2`, `PalwDeltaEntryV2`, the root
preimage or carriage tails without a Lead commit that adds the skeleton first") is honoured by putting the skeleton (the variant,
the two delta entries, the tail and the root block, all dormant) in **one separate commit** that the Lead can take ahead of the
engine.

## 2. The four budgets of one bond (ADR-0176 D1–D2)

### 2.1 Units

| Dimension | Unit | What one claim reserves (its *ask*; the writer computes it) |
|---|---|---|
| `Q` claims | count | 1 |
| `B` reward blocks | fixed point, `PALW_BUDGET_BLOCK_UNIT_V1 = 1_000_000` per physical reward block | the physical reward blocks the claim can cause: an Attempt lead 1 block, a rider 0 (it rides the lead's block), a free-prompt claim `quanta` blocks (one receipt block per spend) |
| `R` reward | base units (BILI × 10^8) | the most the claim can be paid on every leg (producer, seats, reserve, buyback): an Attempt its `escrowed_reward`, a free-prompt claim `quanta × worker carve` at commit (the subsidy never rises) |
| `F` Final weight | the state's weight units (`safe_weight`) | the contribution its Final would add (`palw_claim_safe_contribution_v3` → `palw_weight_final_safe_v1`); free-prompt `quanta × palw_fp_spend_weight_v1` |

**Block rights are physical and never fractional at creation.** ADR-0176 D2 forbids issuing `1/m` of a block. A bond's `B` window
counts whole reward blocks (`U` each). A rider batch shares the lead's one block: when riders attach, the lead's block is
*re-attributed* in fixed point. Each of the `n` riders gets `⌊U/(1+n)⌋`, and the lead keeps `U − n·⌊U/(1+n)⌋`, so the remainder
goes deterministically to the lead and the sum is exactly `U`. Fractional rights therefore exist only as attribution of a
physical block. The bond's window total does not move when riders attach.

### 2.2 The policy (versioned; every value POLICY)

```text
PalwBondBudgetPolicyV1 {
  version                   = 1
  window_daa                W   > 0
  capital_unit_sompi        u   > 0     the capital quantum the per-unit rates are quoted against
  rho                       ρ   ≥ 1     the common claim-capacity multiplier
  claims_per_unit           q   > 0     Q per u per W at ρ = 1
  block_units_per_unit      b   > 0     B per u per W (fixed point, U = one block)
  reward_per_unit_sompi     r   > 0     R per u per W
  final_weight_per_unit     w   > 0     F per u per W
  max_open_claims_per_bond      > 0     the separate outstanding-claim cap (RFC-0015 §8.3.1)
  slice_rights_by_rho       bool        D2's "A → A/m" per-claim ceiling on R and F (POLICY, §10 P-3)
}
```

The fence value is `PalwBondBudgetFenceV1 { activation, policy }`. It is hashed Some-only into both fingerprints, value included,
exactly as `palw_panel_free_v1` hashes its terms. **No value is approved.** The constructor a fence probe uses
(`PalwBondBudgetPolicyV1::unapproved_probe_v1`) is named for what it is. Its numbers exist only so that hashing and validation
have something to read, and they are not a proposal.

### 2.3 Caps, per-claim ceilings and sub-additivity

With `C` the bond's locked capital (`PalwBondStateV2::collateral`, already net of slashes), integer arithmetic in `u128`, floors
everywhere:

```text
Q_max(C) = ⌊C · q · ρ / u⌋        B_max(C) = ⌊C · b / u⌋        R_max(C) = ⌊C · r / u⌋        F_max(C) = ⌊C · w / u⌋
```

* **ρ moves Q only.** `B_max`, `R_max` and `F_max` do not read ρ (D1 MUST NOT). This is the whole "rho → Q/B/R/F mapping" the
  engine implements; the numbers in it are POLICY.
* **The ceilings are equal for honest and forged work.** No cap reads class, PWU, time, tokens, claimed work or speed. A forger
  who claims fast reaches the same `Q`, and a reservation can only be large if the claim's ask is large, which is capped by the
  same totals.
* **Optional per-claim ceilings (D2, `slice_rights_by_rho`).** `R_claim = ⌊r / (q·ρ)⌋` and `F_claim = ⌊w / (q·ρ)⌋`. These
  satisfy `Q_max · R_claim ≤ R_max`, since `⌊Cqρ/u⌋ · ⌊r/(qρ)⌋ ≤ Cr/u` and the left side is an integer. So a bond that fills `Q`
  stays inside `R` and `F`. `B` is never sliced, because blocks are physical (§2.1).
* **No minimum credit and no rounding up.** For any split `C = C₁ + C₂`, `cap(C₁) + cap(C₂) ≤ cap(C)` holds for every dimension,
  because `⌊x⌋ + ⌊y⌋ ≤ ⌊x + y⌋`. Splitting capital across bonds can never raise the total (RFC-0015 §8.3.4). A property test
  checks this over random splits.
* **Slashed capital shrinks the caps at once.** A bond whose window already exceeds the new cap admits nothing until enough
  reservations leave the window. Nothing is taken back, because it was earned under the cap of its time.

### 2.4 The common clock `reuse_not_before = d + W` (ADR-0176 D4)

* The window is rolling and **keyed on acceptance**. A claim accepted at DAA `d` counts against its bond from `d` until the first
  block whose DAA is `≥ d + W`. That block releases it before any admission in the same block (a release queue
  `(reuse_not_before, claim)` in the state).
* **Nothing else releases a reservation.** An early Valid, audit or Final, a cancellation, a void or conviction, a retry, a
  re-send or another session never returns window room before `d + W`. A forfeited reward's reservation stays counted (RFC-0015
  §8.3.3). The window counts the *reserved* vector, and consumption is a subset of it. Consuming therefore moves nothing in the
  window, and recovery has exactly one path.
* **Attribution is by acceptance (DESIGN, §10 D-1).** A claim whose payout or Final comes after `d + W` is still paid from its own
  reservation, attributed to the window of `d`. Over any span of length `W`, a bond's *payments* are bounded by
  `R_max · (1 + ⌈L/W⌉)`, where `L` is the longest claim lifetime (the liability horizon). This is a bounded sum, never the
  unbounded accumulation RFC-0015 §8.3.2(3) forbids. The alternative, holding the room until `max(d + W, terminal)`, is stricter
  and is listed for the user.
* **Liability is a separate ledger.** The budget does not reserve collateral. The existing reservations (`reserved_exposure`,
  escrow terms, kernel, onboarding and provider-court reservations) keep doing that, and admission still needs their headroom
  (the exposure ceiling in `apply_attempt`). Two rules prevent the same collateral from backing old and new claims at `d + W`:
  1. reaching `d + W` frees *issuance room* only; the liability reservations of old claims stay until their own clocks end;
  2. the window is a **withdrawal hold**. `palw_bond_backs_live_duty_v1/v2` also return `true` while the bond has any reservation
     still in its window, so the capital that earned a window cannot leave, or be re-registered, until that window has passed.

### 2.5 Reservation at acceptance

```text
reservation = ask, clipped component-wise by
                (a) the per-claim ceilings when slice_rights_by_rho,
                (b) past palw_model_bond_allocation_v1: R by the model's available budget (§3.4)
admitted iff  open_claims < max_open_claims
          and window + reservation ≤ caps(C) in all four dimensions
```

A bond-level shortfall **refuses** the claim, by path:

* own attempt: the error `BondBudgetExhausted` joins step 4's skip arm. The block stands, carries no claim, and its carve is
  withheld as a skipped attempt's (never minted, `palw_v2_skipped_own_attempt_carve`);
* merged attempt: skipped generically, and the carve withheld by B-1;
* rider: the batch is refused (`CapacityRiders`);
* free-prompt commitment: refused by name.

The check runs before the path's first write, so a skip restores nothing. A skipped own attempt's carve is withheld only past
`palw_audit_2026_09_23` (`palw_v2_skipped_own_attempt_carve`), so validation requires that fence at or below `palw_bond_budget_v1`:
below it, a refused attempt's block would be paid its whole worker carve. A model-level shortfall only **clips `R`**. The claim
is admitted with the smaller reward reservation, and its weight and count are untouched (§3.5).

### 2.6 Consumption (payout, maturity, Final, reward blocks)

`consume(claim, dim, ask, mode)` grants `min(ask, reserved − consumed)` and records it. There are two modes:

* **Clip**: Final weight, Final reward (all legs) and vesting. The writer pays or credits the grant, and the rest is never named.
  For reward this is the same as a work-price remainder, never minted. For weight, it is never credited.
* **Strict**: a reward block, such as a free-prompt receipt spend. Its worker share is paid whole by the coinbase, so a grant
  below the ask refuses the spend. The fold returns the spend's error, and the processor's admission pre-check refuses it first
  (§4, row 3).

The Final reward is clipped **before** the existing split: buyback slice, panel pool, producer and reserve. Every leg then derives
from the clipped amount. This keeps D2's rule that moving reward into another payment name cannot escape the cap. The clip
happens before the vesting row is written, so vesting, maturity, conviction burn and the payout queue need no change.

### 2.7 Terminal claims, retirement and reversal

A claim that reaches Final, void or conviction, or that retires, is *closed*: `open_claims` falls by one and no further
consumption is accepted. Its reservation stays in the window until `d + W`, and the row is deleted at whichever of closing or
release comes later. Reversal of a Final (`reverse_convicted_final`) or retirement of weight (`retire_claim`) refunds nothing to
the budget, so no early recovery comes through those writers.

### 2.8 Split, re-registration, key change, transfer (D4, RFC-0015 §8.3.4)

Bonds are UTXO outpoints with fixed collateral, and a key change or a transfer is a new bond. History survives because the new
bond can only be funded after the old bond's capital is withdrawn, and §2.4's withdrawal hold blocks that withdrawal while any of
the old bond's reservations are in the window. A split therefore yields bonds whose windows start empty only after the old
window has emptied. Over any `W`-span, the combined reservations of the old bond and its successors are bounded by `cap(C)`,
using sub-additivity (§2.3). Bond identity is the outpoint, and no rule reads the operator key for budget purposes. Operator-level
aggregation is not needed for this bound and is not added.

### 2.9 Old claims and the new ruleset (ADR-0176 §4 migration)

* A claim accepted below the fence has no budget row and completes under the rules it was accepted under. Its Final and payout
  writers see no row and run unchanged.
* **The fence's first block seeds the window** from every live (non-terminal) old claim, at its own `accepted_daa`:
  `reuse_not_before = accepted_daa + W`. If that point has already passed, the claim is not seeded. Its *ask* becomes a
  `Legacy`-origin reservation that is never consumed, because old claims are paid under old rules. A bond whose old claims fill
  its new window admits nothing new until they leave it. The upgrade therefore never grants a fresh full window on top of old
  liabilities (ADR-0176 §4: "upgradeを満額枠の再取得経路にしない"). Seeding is one pass over the live claims, once, in the block
  that creates the engine (`None → Some`).

## 3. Model-bond allocation (ADR-0177 D3–D5)

### 3.1 Capital assignment (tag 140, signed)

```text
BondCapitalAssignedV1 {
  bond: PalwBondKeyV2,
  assignments: Vec<(model: Hash64, amount: u64)>,   strictly ascending model ids, every amount > 0, at most max_models_per_bond
  sequence: u64,                                    strictly above the bond's last accepted sequence (replay)
  signature: Vec<u8>,                               the bond key's ML-DSA-87 over palw_capital_assignment_message_v1(network, bond, assignments, sequence)
} = 140
```

* The object **replaces** the bond's pending assignment. `Σ amounts ≤ C_b` (the bond's locked capital), checked at acceptance and
  again at every snapshot.
* Each model must exist on this chain: a class id in `classes` or a line or registration id in `model_lines`.
* It is accepted from **Active** bonds only. Signature, existence and sum are checked; a failure drops the object and the block
  stands. Capital is never derived from claim reservations, counts or ρ, so `S_m` cannot be multiplied by issuing claims.
* Nothing in the object or the snapshot reads Position, AMM liquidity, market seeds, Panel collateral, distribution contracts or
  any availability fact (D1, D3).

### 3.2 Snapshot per allocation epoch, seasoning, decreases

Epoch `t = ⌊(daa − activation) / E⌋` (POLICY `E`). The first block of epoch `t` takes the snapshot:

1. A pending assignment becomes *effective* only after it has stood through `seasoning_epochs` (POLICY, ≥ 1) full epochs. This
   resists short-term borrowing and locking just before the boundary (D3).
2. A pending **decrease** applies at once. The amount used for `t` is `min(effective, pending)` per model, so withdrawing capital
   from a model is never delayed into a larger allocation.
3. A Retiring or absent bond contributes 0. If the bond's capital fell (a slash), its amounts are scaled **pro rata** by
   `⌊C_{b,m} · C_b / Σ_m C_{b,m}⌋`, which keeps `Σ_m C_{b,m} ≤ C_b`.
4. `S_m = Σ_b C_{b,m}`, `A_m = f(S_m)`, and `ΣA` is stored with the epoch.

Snapshot work is bounded by the number of assignment rows. Each row cost a carried, signed object, and each holds at most
`max_models_per_bond` entries.

### 3.3 The allocation curve `A_m = S_m^α` (ADR-0177 as revised 2026-10-10; POLICY)

The goal changed on 2026-10-10. The allocation now strongly favours models with more effective locked miner bond. It no longer
favours publication itself, and equal capital gets equal treatment whoever owns it. The piecewise-linear `f` of the old goal
(policy version 1) was never armed. It is removed, and version 1 is refused.

**The curve.** `A_m = ⌊S_m^α⌋` and `R_m = ⌊R_PALW · A_m / Σ_j A_j⌋`.
* `S_m` is in base units of capital.
* `α` is set in half steps: `alpha_halves = 2α`, in `2..=8`, so `α ∈ {1, 1.5, …, 4}`.
* The interim value is `α = 1.5` (`alpha_halves = 3`), the user's pick in ECON round 3. It is unapproved. It is the preset of
  `unapproved_probe_v1`, and the fence stays refused.
* The tests run at `α = 1`, `1.5` and `2`.
* There is no saturation and no per-model cap. The bonds' own Q/B/R/F caps bound what a claim is paid.

**Exact arithmetic.** `A_m = isqrt(S_m^(2α))`.
* `S_m` is clamped to 64 bits. The clamp is unreachable, because the supply is below `2^64`.
* `S_m^(2α)` is an exact integer below `2^512`.
* `isqrt(n) = max{r : r² ≤ n}` uses Newton's method from above (`palw_isqrt_u512_v1`). It returns the floor of the real `S_m^α`.
  For an integer `α` that is `S_m^α` exactly.
* Weights and their sum are 512-bit integers (`kaspa_math::Uint512`). Each weight is below `2^256`. Their sum over any number of models
  is below `2^320`, and so is `accrued · A_m`. No step saturates or rounds except the documented floors.

**Remainder rule and the budget bound.**
* There are exactly two floors: the weight `⌊S^α⌋` and the share `⌊R·A_m/ΣA⌋`.
* `Σ_m ⌊R·A_m/ΣA⌋ ≤ Σ_m R·A_m/ΣA = R`. So the allocation never exceeds `R_PALW`.
* The remainder `R − Σ R_m` is below the number of models. It is allocated to nobody and never minted (P-7).

**Splits never gain, at any `α ≥ 1`.**
* `⌊x^α⌋ + ⌊y^α⌋ ≤ ⌊x^α + y^α⌋ ≤ ⌊(x + y)^α⌋`.
* The share `x / (x + B)` grows with `x`.
* So the same capital spread over two models never out-weighs it on one.
* A bond split is invisible, because `S_m` is the assigned capital whoever holds it.

**Unchanged.**
* The individual Q/B/R/F caps.
* Distinct capital: no double counting; seasoning stops epoch-edge moves; amounts are clipped pro rata.
* The common hold `d + W`.
* The total budget.
* Nothing scales block issuance, the beacon, Final weight or fork choice.
* A capped bond gains nothing from a steeper curve (test `a_capped_bond_gains_nothing_more_from_a_steeper_curve`).
* Large-capital concentration, adversarial capital included, is accepted as residual risk (ADR-0177 revision).
* **`p = 0` compute-skipping is a separate, unresolved gate.** This curve is not evidence for it.

**The verification-attestation gate** (ECON round 2, readiness §3f) is not implemented on this branch. If it lands, it is a separate
optional policy switch, off by default, and it is not evidence for `p = 0` either. Its design is in ECON §5c.
* Draw: `m` verifiers by stake from the M\*-49 pool, seeded by the RFC-0007 Part VI v3 beacon.
* Pay rule: `k`-of-`m` attestations release the model-epoch subsidy.
* Hold: Final reward is held in vesting until the gate resolves, and burned (never minted) on withholding.
* Allocations: tags 141–143, deltas 194–196 and RPC 254–255 stay reserved for it.

### 3.4 The realized PALW budget and a model's available budget

* `R_PALW(t)` is accrued as it is realized. Each chain block adds its worker carve (`worker_carve_at(subsidy, carve)`) to the
  epoch's accumulator, before admissions. That is the existing carve (ADR-0042 D10, `palw_reward_v2.rs`) inside the fixed
  subsidy, so **nothing new is minted** (D4).
* `available_m = ⌊accrued · A_m / ΣA⌋ − reserved_m`. This is `0` when `ΣA = 0` (the zero-denominator rule) or `A_m = 0`. Floors
  give `Σ_m ⌊accrued · A_m/ΣA⌋ ≤ accrued`, so the sum over models never exceeds the realized budget (conservation, property-tested).
* A claim of model `m` reserves `min(R ask, available_m)` and adds it to `reserved_m`. The model and epoch are fixed on the claim's
  row at acceptance, so a later change in `S_m` never adds rights to a past claim (D5).

### 3.5 Three budgets at once (ADR-0176 "支払時はnetwork/model/bondの全予算を同時に検査")

A payment is `≤` all of the following:

1. **network**: the claim's own carve. It is escrowed per claim, so the payments summed over claims cannot exceed the summed
   carves;
2. **model**: the claim's reservation already took `≤ available_m`;
3. **bond**: the reservation fit the bond's window caps.

The single consumption check against the claim's reservation therefore enforces all three, and it runs only for a claim that is
Final (or a spend the claim's Final licensed). A model shortfall clips **reward only**. The claim, its count and its weight are
admitted exactly as without allocation, so allocation never changes cadence, ticket, DAA, difficulty or fork choice (D5, RFC-0015
§8.5.2).

### 3.6 Unallocated and unused budget

The v1 rule is **not minted**. The part of `accrued` no model can use (`ΣA = 0`, models with no claims, floor dust) and every
clipped remainder follows the existing "withheld, never minted" path. No automatic redistribution above any bond's cap exists
(D4). Whether a later ruleset falls back to the old per-block carve when `ΣA = 0` is a POLICY question (§10 P-7). Under v1, an
armed allocation fence where nobody has assigned capital pays **no** PALW reward.

### 3.7 Which id is a claim's "model"

An Attempt (and a rider) maps to the line that owns its `(class, artifact_root)`: `artifact_line_of_root`. Under ADR-0175 this is
the immutable registration id. Without an owner it maps to the class id, which is also the founding line's id. A free-prompt
claim has no root on the claim and maps to its class id. **DESIGN question §10 D-4:** whether allocation should be per
registration or per class.

## 3b. Round rights before the draw (additional acceptance 2026-10-10, readiness §3e)

[`docs/palw-round-exec-additional-acceptance-2026-10-10.md`](../../palw-round-exec-additional-acceptance-2026-10-10.md) §2–§3.
The existing ticket code (`palw_execution_quanta_v1`, armed on testnet-12) is unchanged. Past `palw_bond_budget_v1` the span's
schedule is minted by a **capped** variant, and below the fence it is minted exactly as before.

```text
T_earned(final)  = palw_execution_quantum_count_v1(credit, 100,000, seed, final)   (verified CanonicalWork; the seed rounds the remainder)
R_round(bond)    = the bond's remaining Round rights in its window, read just before the draw
T_candidate(b)   = Σ over b's Finals, in canonical order, of min(T_earned, what is left of R_round(b))
T_allocated      = the seed's draw H(seed ‖ quantum_id) over every candidate ticket, the first `window_rounds` of them
T_executed      <= T_allocated  (a permit exists only for an allocated ticket)
Σ T_allocated per window <= window_rounds (the shared capacity, 120 on testnet-12 — never a per-claim grant)
```

**The versioned specification (`PalwRoundRightsPolicyV1`, inside the budget policy):**

| Item | v1 |
|---|---|
| Unit | one ticket = one Round right = at most one algo-10 permit |
| Period | the bond's common window `W`, keyed on the **draw** DAA |
| Reserve | at the draw (the span's first chain block), `T_allocated` of the bond, as one budget row per `(span, bond)` |
| Consume | at the same moment: an allocated ticket is a used right, executed or not |
| Release | at `draw DAA + W` (the common clock), and at no other time |
| Expiry | an allocated ticket whose round passes unspent is lost; its reservation stays until `draw + W` (no refund) |
| Earliest reuse | `draw DAA + W` |
| Remainders | the existing seed-drawn stochastic rounding of `credit / 100,000` per Final; the capital cap floors |
| Lost draws | a candidate not drawn is not reserved and carries nothing forward |
| Carry-over | none: a Final's unallocated tickets do not move to a later span (the existing mint's rule) |
| Concurrent windows | the cap reads every reservation still in the window (other spans' draws included), so concurrent spans cannot exceed it |
| Binding | the same bond and capital as claims, receipts and roots: one `C`, one window, one release clock |

**Fee-only Rounds (POLICY P-9; both modes are implemented, and neither leaves an uncapped path):**

* `CountAgainstBlocks`: each allocated ticket reserves one reward block (`U`) of the bond's `B`. Fee-only Rounds then compete with
  attempts and receipts for `B_max`.
* `ExecutionCap { rights_per_unit }`: a separate dimension of the window, `⌊C · rights / u⌋` tickets per `W`.

**Market fee income and `R_max` (POLICY P-10).** Fee income depends on the market and cannot be reserved before it exists. v1 does
not count it in `R`. What bounds a bond's fee-earning opportunities is the Round-rights cap above, which every permit passes. The
alternative, counting fees in `R` at receipt and burning the excess, is listed for the user.

**SPLIT / NEUTRALITY.** For the same work and the same total locked capital, the expected candidate total cannot rise:

* splitting capital across bonds or operator keys gives `Σ ⌊C_i·r/u⌋ ≤ ⌊C·r/u⌋`;
* splitting a claim or resubmitting work sums the same credits: the stochastic rounding is unbiased, and a Final's copies collapse by
  `execution_root` (one ticket set per work);
* repackaging as root/slice credits the root only (X8R's contract);
* `Σ_i min(T_i, R_i) ≤ min(Σ T_i, Σ R_i)`.

The capped mint therefore drops the old per-Final `min(window)` cap. That cap let a split into `k` claims field `k · window`
candidates where the whole fielded `window`. The draw ranks tickets by `H(seed ‖ quantum_id)`, which is uniform, so the expected
allocation is proportional to candidates.

**Tolerance (fixed):** per bond, the realized candidate count of a split differs from the whole's by at most one rounding ticket per
Final and one floor ticket per piece of capital, with expectation ≤ 0 (never in the splitter's favour).

The draw reads no operator id, domain, genesis flag or registration order. The role-swap test swaps two bonds' labels and gets the
swapped allocation. A span's candidates are bounded by `Σ_b R_round(b)`, and the hard per-span bound
`PALW_EXEC_MAX_QUANTA_PER_SPAN_V1` must not bind under an approved policy (POLICY: `Σ caps < 65,536 per window`), because it
truncates in root order.

**Order of the step.** The budget's clock (§2.4) runs before the execution lane's span rotation in every block, so a draw sees this
block's releases. Validation requires `palw_economic_safety` at or below the budget fence: the windowed mint is the one capped.

## 4. Inventory: every reward-bearing writer and reader on this tree

`S` = `consensus/core/src/palw_state_v2.rs` @ `b8ae9412b`; `P` = `consensus/src/pipeline/virtual_processor/processor.rs`.
Columns: what it writes today → reserve at acceptance → consume at block / payout / maturity / Final → where.

| # | Path (today) | Reserve (Q, B, R, F) | Consume | On this tree |
|---|---|---|---|---|
| 1 | **Attempt claim** — `apply_attempt` S:40060 records `reserved`, `escrowed_reward` (worker carve), `immature_contribution`, issuance slot/bucket, emission charge, economics snapshot; `accepted_daa` = accepting chain block's DAA | Q 1; B `U` (its carrying block); R `escrowed_reward`; F the Final contribution. Immature weight clipped to `F` | B at acceptance (the block exists); F at `finalize_claim` S:27817 (clip); R at `finalize_claim` before the buyback/panel split (clip) | wired (M3) |
| 2 | **Merged attempt** (B-1) — same arm via step 4b | as 1 | as 1 | wired (M3); refusal skips; carve withheld by B-1 |
| 3 | **Riders** (tag 95) — `attach_riders_v1` S:39952 cuts the lead to the remainder and admits each rider at `⌊E/(1+n)⌋` with full immature/Final weight | rider: Q 1, B 0, R `e_r`, F its contribution; lead's R re-sized to its kept escrow in the same transition; B re-attributed `⌊U/(1+n)⌋` per rider, remainder to the lead | as 1 | wired (M3). Closes inventory gap "riders multiply weight": each rider's F is reserved |
| 4 | **Free-prompt commitment** — `FreePromptCommitted` S:37312: `reserved`, `rights_reserved`, `immature = 0`, `escrowed = 0`; Final adds no weight | Q 1; B `quanta·U`; R `quanta × carve`; F `quanta × per-quantum weight` | each spend (row 5) | wired (M3) |
| 5 | **Receipt V3/V4 spend** — `apply_receipt_spend` S:38060 adds per-quantum weight to `safe_weight`; the receipt block's worker share is paid whole by the coinbase (V4 splits the same total into miner + builder legs, `palw_receipt_v4_split_v1`) | (from 4) | B `U` strict, R the block's carve strict, F per-quantum clip | wired in the fold (M3). The admission pre-checks `check_palw_receipt_spend_admission_v3/v4/v5` need the same strict check (hook H-5). Closes gap "rights released at Final before the spends" |
| 6 | **Receipt licence batch** (`ReceiptLicensedBatchV1`) — routing only, no reward of its own | — | — | nothing to wire |
| 7 | **Final** — `finalize_claim`: `safe_weight += palw_weight_final_safe_v1(…)`; escrow → economics price → buyback → panel/producer/reserve legs → payout row or vesting row | — | F clip, R clip (all legs) | wired (M3) |
| 8 | **Vesting / maturity** — `write_vesting_row_at_final` S:28615, `latch_matured_vesting_rows` S:28724, `apply_vesting_move`, `burn_vesting_row` | — | none: legs are already clipped at Final; a burn refunds nothing | unchanged |
| 9 | **Immature weight** — `reserve_for_claim` S:27213 (old rule) / `weight_cap_on_write` (new rule) | F bound | the claim's `immature_contribution ≤` reserved F | wired (M3) |
| 10 | **Retire / reversal** — `retire_claim` S:28150 → `retired_safe_weight`; `reverse_convicted_final` S:24113 | — | none (no refund) | close hook (M3) |
| 11 | **Void / conviction / default** — `void_claim` S:28011, `void_and_slash_at` S:27429 | — | none; closes the claim | close hook (M3) |
| 12 | **Coinbase** — `palw_v2_escrow_withheld_at` P:6555, `palw_v2_merged_escrow_withheld` P:6638, `palw_v2_payout_outputs` P:7122; payout queue `write_payout` S:20076 | — | reads the clipped rows; withholding unchanged (a refused attempt is a skip) | unchanged by construction |
| 13 | **Emission budget / breaker (ADR-0167, F-EM/F-K)** — `palw_emission_admits_v1`, 16 carves per DAA | the network budget, kept | — | unchanged (it is the network layer of §3.5) |
| 14 | **RFC-0006 shard segments** — seat locks per part; `final_legs_v1` splits the panel pool by shares at Final | — | inside the Final R clip (the pool is a slice of the clipped reward) | covered by row 7 |
| 15 | **Kernel route Final reward** — `misaka-palw-kernel` `FinalReward` (`ledger.rs` tick) → `apply_settlements` → `add_kernel_payout`; on `g14/r4-fixes` paid from the poster's job escrow (GAP-5) | kernel claim commit must reserve | FinalReward clip | **hook H-1** (dormant route; G14R branch) |
| 16 | **OPV rewards / seal deposits / bounties** (`g14/r4-fixes`, `opv/bootstrap-beacon`) | producer rewards: as 15; bounties and deposits are slash-funded or deposit flows, not producer issuance | — | **hook H-2** (bounties outside ADR-0176; the producer FinalReward inside) |
| 17 | **RFC-0004 typed roots** (spec jobs, `spec_on_final`) | as 15 | as 15 | **hook H-1** |
| 18 | **RFC-0008 EXEC v2** (`rfc8/x8r-review`): `settle_at_final_v2` splits the root claim's reward leg among slice executors; EXEC_TX/EXEC_SLICE weight and DAA stay 0 | slices reserve nothing new | slices are paid from the root's clipped Final reward; a block whose coinbase attributes a worker share outside an escrowed claim must consume `U` from the bond window: `palw_bond_budget_consume_block_v1` | **hook H-3** |
| 19 | **Model allocation** (ADR-0177) | the R clip of §3.4 | — | implemented (M2), wired (M3) |
| 20 | **Fork choice** — `candidate_order` S:14216 (frontier, `safe_weight`, `bounded_immature`); `palw_fork_authority_v2` | — | reads the clipped weights only | unchanged by construction |
| 21 | **DAA** — PALW weight does not feed DAA or `blue_work` (`palw_chain_weight.rs`); per-class DAA retargets from `epoch_counters` / `receipt_epoch_counters` (production census) | — | the census counts produced work, not reward; unchanged. Allocation never touches it (§3.5) | unchanged |
| 22 | **Rule E (ADR-0178, FINX branch)**: its readers of Final weight | — | must read the *budgeted* Final weight of a claim (`PalwChainStateV2::bond_budget_final_weight`) where a row exists | **hook H-4** |
| 23 | **RPC readers** — `get_palw_claims_call`, `get_palw_vesting_call`, `get_palw_settlement_call`, `get_palw_class_economics_call`, `get_palw_capacity_shadow_call`, `get_palw_kernel_*` | — | read clipped state; the budget itself through ops 250–253 (§8.4) | designed; ops not implemented |
| 24 | **EVM / settlement** — `palw_settlement_v1` (sink claims), `native_delta_evidence_v1` (deltas: claims entering Final, FP spends, voids), `evm_settlements` / `MarketSettle` | — | read Final phases and spends only; amounts already clipped | unchanged |
| 25 | **Undo / reorg / IBD / pruning** — per-block `PalwStateDeltaRecordV2`; `revert_delta_v2`/`apply_delta_v2`; pruning snapshot and IBD import of the carriage, checked against the child header's root | — | budget rows are deltas 190/191; the engine rides tail 0xEF and the root block | implemented (M2) |
| 26 | **Reporter rewards, slash, burns** — 49% of the collected slash (ADR-0032) | not producer issuance | — | out of scope (ECON uses the net loss) |
| 27 | **Activation-pool payouts, model fee legs, improvement grants** | funded by top-ups and sinks, not the subsidy carve | — | out of scope; listed for ECON (§10 P-6) |

## 5. State, deltas, carriage, root

```text
PalwChainStateV2::bond_budget: Option<PalwBondBudgetStateV1>       None below palw_bond_budget_v1 (and on every preset)
PalwBondBudgetStateV1 {
  header:      PalwBondBudgetHeaderV1 { version, created_daa, policy_digest, allocation: Option<PalwAllocationEpochV1> }
  bonds:       table 1  PalwBondKeyV2 → PalwBudgetBondRowV1 { window: Vector, open_claims }
  claims:      table 2  Hash64 → PalwBudgetClaimRowV1 { bond, model, epoch, accepted_daa, reuse_not_before,
                                                        reserved: Vector, consumed: Vector, open, in_window, origin }
  releases:    table 3  (reuse_not_before, claim) → ()
  assignments: table 4  PalwBondKeyV2 → PalwCapitalAssignmentRowV1 { sequence, pending, pending_since_epoch, effective }
  models:      table 5  Hash64 → PalwModelBudgetRowV1 { epoch, s_m, a_m, reserved_sompi }
}
```

* **Deltas.** 191 creates and updates the header; 190 writes or drops one Borsh row of one table, verify-then-install, the same
  shape as the kernel route's 160/161. `apply_delta_v2`/`revert_delta_v2` gain two arms, and a delta-number pin covers both.
* **Root.** Only when `Some`: `b"bond_budget/v1" ‖ H(header) ‖ rows_root`, after the kernel route's block. A state without the
  engine has exactly the root it had.
* **Carriage.** Tail 0xEF, present only when `Some`, decoded once and refused if repeated. A carriage without it is byte-identical.
* **Unarmed byte-identity.** With the fence absent: the field stays `None`, no delta 190/191 is written, no root block or tail
  appears, every reader answers "no row", and every hook is the identity. M2 proves this with the existing golden vectors
  unchanged and with a fold run of the shared fixture whose deltas, roots and carriage bytes are equal with the fence `None` and
  with it armed at an unreached height.

## 6. A-2: tag 140 rides unjudged below its fence

* `palw_lifecycle_object_may_ride_v2` answers `Ok(())` for tag 140 with **no shape check** (no "unsigned → invalid"; A2U finding
  #1). A size bound is checked only at and above the fence, and only by dropping.
* The acceptance walk drops tag 140 **by name, first and charged nothing**, below `palw_model_bond_allocation_v1`, exactly as the
  DA16 and kernel-route kinds are dropped. Above the fence a bad signature drops it; the block stands.
* The fold refuses tag 140 below the fence as the second lock. Rent ceiling is 0, and it is not a chunked kind.
* For A2U's central table: **row "tag 140 → `palw_model_bond_allocation_v1`"**. On a ruleset without the audit fence int-12
  refuses undecodable bytes, and the central rule must make this build refuse tag 140 there too. That is A2U's mechanism, listed
  for the Lead (H-6).

## 6b. PESG §6: safety bounds independent of the economics (round 3, 2026-10-10)

These bounds come from `probabilistic-economic-security-gate.md` §6, with values from ECON §5e.4. All of them sit behind
`palw_bond_budget_v1`. The budget policy is now version 2 (version 1 is refused).

| Bound | Rule | Where it is enforced |
|---|---|---|
| No payout before Final, on any leg | No producer row in the payout queue, vesting row (which names the seat and model-allocation legs) or execution-lane Final for a claim that is neither Final nor voided. FP receipt spends already require the FP claim's Final. The verifier bounty is paid from the collected slash, after the conviction. | `PalwChainStateV2::bond_budget_no_payout_before_final_v1`, run with the import consistency check. Every writer of those rows runs at Final. |
| External export cap | Nothing leaves before Final. After Final, while the liability holds collateral, what leaves at once is at most `⌊export_cap_permille · K / 1000⌋`. The cap is at most 510 permille (0.51), and `K` is the claim's `reserved`. "What leaves at once" means a leg written straight into the payout queue (no vesting), a buyback slice in a market reserve, or the sum of FP receipt spends while `now < final + H_L`. A vesting row is not an export, because it moves only after its lock (the liability) has ended. The rest is never named, so never minted. Legacy claims are paid by their old rule. | `finalize_claim` (non-vesting legs and buyback); `bond_budget_receipt_spend_v1` plus the processor's H-5 pre-check (`bond_budget_spend_fits_v1`, now given params and DAA). |
| Open claims per bond | `Σ K` over open claims `≤ C`, so at most `⌊C/K⌋` open claims of reservation `K`. Checked at acceptance. | `palw_bond_budget_admit_v1` → `LiabilityExceedsCapital` |
| Unsettled weight per bond | `Σ` Final-weight reservation over open claims `≤ F_max`. This counts claims still open past `d + W`, which have left the window but not the unsettled weight. | `palw_bond_budget_admit_v1` → `UnsettledWeight` |
| Value of weight | A versioned slot, `PalwWeightValuePolicyV1`. It ships `Unknown`, so weight is bounded by `F_max` alone. Setting `SompiPerWeight { v }` also holds `reward + F·v ≤ R_max`. | admission |
| `W` | Still POLICY. Validation keeps it within 280..=436 DAA, ECON's derived range. The fixtures run below validation. | `PalwBondBudgetPolicyV1::validate` |
| Liability hold `H_L` | A clock of its own: `liability_hold_daa`, interim 280. It runs from an open claim through to terminal + `H_L`, apart from the issuance clock `d + W`. The withdrawal hold reads both clocks. | engine table 6 (`liability_releases`); `withdrawal_holds` |

**Export paths not reached by this round:**
* Kernel-route payouts (G14: `add_kernel_payout` in `palw_kernel_route_fold_v1::apply_settlements`).
* Improvement grants (`palw_improve_fold_v1`) and mesh audit payouts (`palw_mesh_fold_v1`).
* Activation-pool payouts and carrier refunds. These are funded by top-ups, not by claim rewards.
* EVM `MarketSettle` trades. These are holders trading positions, and the reserve a claim's buyback adds is capped above.

**G14 hook call sites, for codex.** All of them are in `palw_kernel_route_fold_v1.rs`, `apply_settlements`. OPV and typed-root claims
settle through the same ledger events.
* **`SettlementKindV1::ReserveClaim`:** reserve with `reserve_liable`. Use origin `KernelRoute`, the claim's Q/B/R/F ask and
  `K` = the reserved claim collateral, under the engine's admission.
* **`SettlementKindV1::FinalReward`:** before `add_kernel_payout`:
  * consume `Reward` on the claim's row (as `bond_budget_final_reward_v1` does);
  * then clip to `palw_bond_budget_export_cap_v1(policy, K)`, because kernel payouts do not vest;
  * the rest is never named.
* **`ReleaseClaim`, a conviction, or a timeout:** `close_at(claim, Some(now + H_L))`.
* **`AccuserReward` / `DemanderShare`:** these are paid from the collected slash at the conviction, so no budget hook is needed.
  The import check covers "no payout before Final".

Still missing on integration: EXEC (X8R `consume_block_for_bond`) and the rule-E reader (FINX `bond_budget_final_weight`).

## 7. Hooks other lanes call (names fixed in M2)

| Hook | Who | Call |
|---|---|---|
| H-1 | G14R (GAP-5 escrow Final reward), R4X (spec jobs), K2S | at kernel claim commit: `TransitionBuilder::bond_budget_reserve_v1(claim, bond, model, ask, origin = KernelRoute)`; at `FinalReward`: `bond_budget_consume_v1(claim, Reward, amount, Clip)`, pay the grant, never mint the rest |
| H-2 | OPVB | a producer reward on the OPV route is H-1. Accuser bounties and seal deposits are not producer issuance: no budget, listed for ECON |
| H-3 | X8R | `settle_at_final_v2` already splits the clipped reward. Any block whose coinbase pays a worker share outside an escrowed claim (a claim-backed or EXEC_SLICE reward block) calls `bond_budget_consume_block_v1(bond, now)` and is not reward-bearing on `Err` |
| H-4 | FINX (rule E) | read `PalwChainStateV2::bond_budget_final_weight(claim) -> Option<u128>` (the Final weight a budgeted claim was credited) instead of re-deriving the full contribution |
| H-5 | the FP admission pre-checks (`palw_fp_admission_v3`, `palw_receipt_v4`) | `palw_bond_budget_spend_fits_v1(state, claim, carve)` beside `QuantumAlreadySpent`, so a block the fold would refuse is never templated |
| H-6 | A2U | tag 140 → `palw_model_bond_allocation_v1` in the central kind→fence table |
| H-7 | INTF / ECON | the 49% reporter share and slash flows stay outside the budget. ECON's `L_collectible_net` uses the net loss |

## 8. Readers

### 8.1 Pure readers (M2)

`bond_budget()`, `bond_budget_caps_v1(bond)`, `bond_budget_claim(claim)`, `bond_budget_final_weight(claim)`,
`bond_budget_window_holds(bond, now)`, `model_allocation(model)`.

### 8.2 Withdrawal

The live-duty predicates read `bond_budget_window_holds` (§2.4).

### 8.3 EVM / settlement

These need no reader. The amounts they read are already clipped.

### 8.4 RPC ops 250–253 (designed, not implemented)

| Op | Name | Returns |
|---|---|---|
| 250 | `getPalwBondBudget` | a bond's capital, caps, window, open claims and next release DAA |
| 251 | `getPalwClaimBudget` | a claim's reservation, consumption, `reuse_not_before` and model/epoch |
| 252 | `getPalwModelAllocation` | the epoch, `S_m`, `A_m`, `ΣA`, accrued, `available_m` and `reserved_m` |
| 253 | `getPalwCapitalAssignment` | a bond's pending and effective assignments and sequence |

All four show "fence dormant" until armed.

## 9. Acceptance map (ADR-0176 §4, ADR-0177 §3 → tests)

`E` = engine unit or property test (`palw_bond_budget_v1`, M2). `F` = fold-level test through `apply_palw_transition_v2` (M3).
`N` = real-node E2E (`consensus/src/pipeline/virtual_processor/tests/budget_e2e.rs`, M3). `X` = not provable in this lane.

| Acceptance row | Test(s) |
|---|---|
| **176-1** same capital, window and ρ; honest vs fast forgery; different class/work | E `caps_read_capital_and_policy_only`; F `same_bond_honest_and_forger_reach_equal_ceilings`; N `budget_e2e::honest_vs_forger_equal_ceilings` |
| **176-2** ρ ×1/×100/×1000, fractions, minimum credit, window boundary | E `rho_moves_q_only` (property over ρ and random asks), `split_capital_never_raises_caps`, `slice_ceilings_fit_the_caps`; F `rho_sweep_keeps_brf`; N `budget_e2e::rho_1_100_1000` |
| **176-3** one claim → many receipts/blocks; FP; slice/rider/batch; rights transfer | E `consumption_never_exceeds_reservation`, `rider_block_attribution_sums_to_one_block`; F `free_prompt_spends_draw_the_commit_reservation`, `riders_share_the_lead_block`; H-3/H-1 for slices and kernel |
| **176-4** early Final, void, retry, `d + W`, unresolved court, collateral exhaustion | E `release_only_at_d_plus_w`; F `early_final_void_retry_recover_nothing_before_d_plus_w`, `window_holds_withdrawal`; N `budget_e2e::early_final_void_retry` |
| **176-5** bond split / re-registration / key change / transfer, mixed ruleset | E `split_capital_never_raises_caps`; F `split_and_reregister_wait_for_the_window`, `old_claims_seed_the_window_at_the_fence`; N `budget_e2e::split_reregister` |
| **176-6** Final / retired / reversal, fork choice / DAA, RPC/EVM, restart/reorg/IBD | E `delta_round_trip`, `carriage_round_trip`; F `retire_and_reversal_refund_nothing`, `reorg_by_delta_equals_fresh`; N `budget_e2e::reorg_and_restart_replay_identical` |
| **176-7** Panel/miner collusion, external verification, actual recovery, self-collusion bounty | **X**: G14/ECON. The budget bounds the maximum gain only; `p` and `L_collectible_net` are MEAS/ECON |
| **177 Non-interference** | E `allocation_reads_no_availability_input` (the inputs are the state's capital and assignment rows only); F `allocation_is_independent_of_distribution` |
| **177 Identity / court scope** | **X** (ADR-0175 fence; DA16b/K2S2 court units) |
| **177 Distinct capital** | E `assignments_never_exceed_capital`, `many_claims_do_not_multiply_s_m`, `pro_rata_clip_after_slash`, `seasoning_delays_increases_not_decreases`; F `assignment_replay_and_foreign_bond_refused` |
| **177 Allocation conservation** | E `model_budgets_sum_within_accrued` (property), `zero_denominator_is_zero`, `curve_validation`, `overflow_bounds`; F `unused_model_budget_is_not_minted` |
| **177 Same-bond opportunity** | F `model_budget_never_restores_bond_window` |
| **177 Open vs closed economics** | **X**: ECON (D6); E `same_s_m_same_allocation_whoever_owns_it` covers the engine's half |
| **177 Recovery / activation** | E/F round trips as 176-6; params tests `both_fences_dormant_and_refused` |
| **§3e BUDGET** (unit, period, reserve, consume, expiry, reuse, fee attribution; every writer/reader/undo agrees) | §3b table; E `round_rights_reserve_once_and_release_at_d_plus_w`; F `round_draw_reserves_through_deltas_and_reverts` |
| **§3e WORK** (same bond, light vs heavy work: proportional below the cap, limited at it) | E `candidates_follow_work_below_the_cap_and_stop_at_it` |
| **§3e WINDOW** (120 shared slots saturated, huge candidate sets, capped bonds, concurrent windows) | E `the_window_is_shared_and_concurrent_draws_never_pass_the_cap` |
| **§3e NEUTRALITY** (role swap: operator / genesis / new miner; registration order) | E `a_role_swap_swaps_the_allocation_and_order_is_irrelevant` |
| **§3e SPLIT** (bond / operator / claim splits, rounding, candidate cap, resubmission) | E `splitting_bonds_or_claims_never_raises_candidates`, property over random splits, tolerance asserted |
| **§3e RECOVERY (budget half)** | the round-rights rows ride deltas 190/191 and tail 0xEF: F reorg-by-delta and carriage reload in the Round draw test; real-node IBD/reorg as 176-6 |

## 10. Open questions for the user

**DESIGN**

* **D-1** Attribution by acceptance with payments after `d + W` drawn from the old reservation (§2.4), or room held until
  `max(d + W, terminal)`?
* **D-2** `R` covers the whole claim reward: producer, seats, reserve and buyback. Should seat pay instead count against the
  *seat's* bond?
* **D-3** A free-prompt spend whose reservation is short is refused (strict), not partially paid. A partial pay needs a
  coinbase-withholding rule for receipt blocks.
* **D-4** The model key: the immutable registration (line) id, or the class id (§3.7)?
* **D-5** A rolling window keyed on acceptance (implemented), or fixed epochs? The ADR allows either.

**POLICY** (none set; the struct carries them)

* **P-1** `W`.
* **P-2** `u`, `q`, `b`, `r`, `w` and the open-claim cap.
* **P-3** ρ and whether rights are sliced per claim by ρ (`slice_rights_by_rho`).
* **P-4** the allocation epoch `E` and its tie to `W`.
* **P-5** seasoning.
* **P-6** `α` of `A_m = S_m^α` (interim, unapproved: 1.5), and the acceptance quantification ADR-0177's revision asks for: the capital range, the multiplier, and whether the advantage holds for allocation, actual payment or net profit below the individual caps.
* **P-7** the rule for unallocated budget when `ΣA = 0` (v1: not minted).
* **P-8** `max_models_per_bond`.
* **P-9** fee-only Rounds: `CountAgainstBlocks` (each allocated ticket one block of `B_max`) or `ExecutionCap { rights_per_unit }`.
* **P-10** whether market fee income counts in `R_max` (v1: no; bounded by the Round-rights cap) or is counted at receipt with the
  excess burned.
* **P-11** `Σ` of Round-rights caps per window below `PALW_EXEC_MAX_QUANTA_PER_SPAN_V1` (65,536).

## 11. Status by milestone

* **M1** (this note): design, inventory and acceptance map.
* **M2**: the dormant engine in consensus-core, with its state, deltas, object and unit/property tests.
* **M3**: wiring rows 1–5, 7, 9–11 and 19 behind the fences, plus fold-level and real-node E2E.

Results are recorded in §12 when each milestone is built.

## 12. Results

Status words: **implemented** (code on this branch), **verified** (a named test passed on a build of this branch),
**armable** (none: every POLICY value in §10 is open, and both validators refuse an armed fence).

Built 2026-10-10 on `budget/adr176-177` after merging integration `f325d6696`, with `BUILDSLOT_SINCE=1760000002`.
The test names in §9 were planned in M1. The rows below name the tests that exist.

| Area | Status | Tests that passed |
|---|---|---|
| Engine: caps, ceilings, ρ, split, clock, reservation, consumption, riders, deltas, carriage | verified | `palw_bond_budget_v1::tests` (27, incl. `rho_moves_q_only`, `split_capital_never_raises_caps`, `release_only_at_d_plus_w_and_closing_returns_nothing`, `delta_round_trip_and_revert`, `carriage_round_trip`) |
| Model allocation (ADR-0177 D3–D5) | verified | `model_budgets_sum_within_accrued_and_zero_denominator_is_zero`, `many_claims_do_not_multiply_s_m_and_assignments_never_exceed_capital`, `seasoning_delays_increases_not_decreases_and_pro_rata_after_slash`, `same_s_m_same_allocation_whoever_owns_it`, `model_budget_clips_reward_and_never_restores_the_bond_window` |
| §3e BUDGET / WORK / WINDOW / NEUTRALITY / SPLIT (engine) | verified | `round_rights_reserve_once_and_release_at_d_plus_w`, `candidates_follow_work_below_the_cap_and_stop_at_it`, `the_window_is_shared_and_concurrent_draws_never_pass_the_cap`, `a_role_swap_swaps_the_allocation_and_order_is_irrelevant`, `splitting_bonds_or_claims_never_raises_candidates` |
| Fences dormant, Some-only hashed, refused when armed | verified | `both_fences_are_dormant_some_only_hashed_and_refused_when_armed`, `fork_id_v1::`, `consensus_params_id_tests` |
| Unarmed fold byte-identical | verified | `the_unarmed_fold_is_byte_identical_whether_the_fence_is_absent_or_scheduled_ahead`; `palw_state_v2::` (whole module), `palw_execution_quanta`, `palw_execution_lane` |
| Fold wiring (attempt, FP, riders, Final, void, retirement, reversal, withdrawal hold) | verified | `palw_state_v2::tests::bond_budget_fold_v1` (12) |
| Sim harness on the real fold | verified | `palw_bond_budget_e2e`: 7 of 7 (honest vs forger, ρ ×1/×100/×1000, early Final / void / retry, ten bonds vs one, allocation, rewind/replay, Round draw) |
| Real node (T12Chain) | verified | `budget_e2e` 4 of 4 (equal ceilings, no early recovery, restart; refusal and ρ; IBD block by block; reorg) |
| A-2 row for tag 140 | verified | `t12_a2u_new_kinds_uniform` 3 of 3; wire pin `(140, 0xa4ab9f7696775588)` |
| Unaffected neighbours | verified | `p2_mint_path`, `rfc9_v4_chain_e2e` |
| RPC 250–253, EVM view | not implemented | §8.4 |

**The Round draw on the real fold** is wired and replays, but no tickets were drawn in it. With an admitted attempt in every
block, 400 blocks seeded one matured snapshot through `palw_execution_mint_quanta_windowed_capped_v1`. That schedule was empty
because a floor Final's CanonicalWork is below one execution quantum (100,000). The 2M class could not stand in on this harness,
because only one attempt was admitted there. So the ticket arithmetic past the fence (candidates bounded by remaining rights,
shared capacity, reserve at the draw and release at `draw + W`) is verified only at the engine level. A fold-level test that draws
non-zero tickets needs a heavier class fixture. It is open.

Failures on this tree that are not this lane's: the `a2u` pins for tags 109 (changed) and 113 (unpinned), and
`dns_finality::TakeoverToken (gone)` missing from `PALW_INT12_WIRE_CHANGES_V1`.

**Round 2 (2026-10-10, ADR-0177 revision).** The curve `A_m = S_m^α` is implemented and verified. There are 8 of 8 Sim tests and
the engine and fold filters pass, built on integration `d4e37155e`. Whether it is armable: no. The tests are:
* `curve_validation_isqrt_and_exact_powers`, which checks the floor root on random 512-bit inputs and exact powers to `α = 4` at
  `S = 2^64 − 1`;
* `two_to_one_capital_is_four_to_one_at_alpha_two_and_two_to_one_at_alpha_one` (2:1, 2.83:1, 4:1);
* `model_budgets_sum_within_accrued_and_zero_denominator_is_zero` (Σ ≤ R, loss below one per model);
* `splitting_a_model_or_a_bond_never_raises_the_total`;
* `a_capped_bond_gains_nothing_more_from_a_steeper_curve`;
* fold `alpha_two_divides_the_realized_carve_four_to_one_and_every_block_replays` (delta, revert and carriage reload);
* Sim `alpha_one_one_and_a_half_and_two_divide_the_budget_by_s_to_the_alpha_on_the_real_fold_and_replay`, which measured
  640,169,300,160 : 320,084,650,080 at `α = 1`, 709,431,897,488 : 250,822,052,751 at `α = 1.5` (2.8284) and
  768,203,160,192 : 192,050,790,048 at `α = 2`.

The verification-attestation gate is not implemented (see §3.3).

`scripts/t12-repin.sh --shipping --drift-only` on this tree: no drift (361 ok, 29 history, 3 label). The testnet-12
params id `5ee7fd8e…` and schedule id `1678e073…` are unchanged.
