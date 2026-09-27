# PALW spec — 10. Collateral and economics

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. The rule
> families B, SR, V, L, A, S, T and W are in ADR-0152 §3.1–§3.8, and the capacity design is in
> ADR-0160. Neither is on this branch.

**Purpose.** Collateral covers fraud, and liveness is structural. A bond does not buy the right to be
live, and a failure is not a verdict. This chapter defines the bond, what a claim reserves until its
licence, the seat lock a resolved claim leaves behind and how long it lives, the one ledger of
committed collateral, the action-based slash schedule and the reporter's reward, vesting of rewards
until the conviction window closes, the throughput caps, and emission: the premine cap, the subsidy
and its carves. The design bar is that the most an absconding participant can gain never exceeds
what the chain can recover.

**Principles served:** P5 (profit from efficiency is kept), P6 (eligibility is not bought with stake
beyond what fraud requires).

## 10.1 The bond

- [ ] B-1 one account, B-2 floors, B-3 exit stays bounded, B-4 no ejection status and no tombstone
  (producer floor gate, `Eq` capped on testnet-12), B-5. *Sources:* 0152 §3.1, 0046 D3 (the bond is its
  collateral output), 0016. *Code:* `core/palw_state_v2.rs` `PalwBondStateV2`, `PalwBondStatusV2`
  (`Active`, `Retiring`); `core/palw_producer_v2.rs` (`PalwBondSummaryV1`, `PalwBondLocksV1`).
- [ ] One key registers one bond for the life of the chain, and operator ids are unique. *Sources:*
  0152, the audit fence `palw_operator_id_unique`.
- [ ] A bond is usable in the block that registers it, but only a mature bond sits on panels.
  Reconcile 0064 with 0065 D1. **Maturity:** 1,000 DAA from registration
  (`PALW_T12_BOND_MATURITY_WINDOW_DAA`). `palw_bond_maturity` is at DAA 1,000. From DAA 750,
  `palw_bond_maturity_early` applies the window to every non-genesis bond, including bonds registered
  before 750, which leave the draw at 750 and return at registration + 1,000. *Sources:* 0065 D1, the
  launch note §00. *Code:* `PalwBondMaturityV1`.
- [ ] Floors: the seat floor and the producer floor, and the values per network (chapter 16).

## 10.2 The staged reservation (SR)

- [ ] SR-1 the one expression, SR-2 recorded, SR-3 one funnel (fence-aware), SR-4 monotone, SR-5 what
  a failed claim forfeits, SR-6 licence on the backed subset, SR-7 admission on live state, SR-8
  `ExecutorRefuted`, SR-9 early redraw on three or more `Unavailable`, SR-10 the supplementary V3
  door. *Sources:* 0152 §3.2. *Code:* `claim_escrow_reservation_v1`, `palw_claim_escrow_v1`,
  `window_bind`, `escrow_backed_exposure_from_daa`; `core/palw_exposure.rs`.

## 10.3 Seat locks (L), and how long they live

- [ ] L-1 priced by the recounted set, L-2 recount (not a flat divisor), L-3 lifetime, L-4 duty
  reserves the lock, L-4b the generalized top-up. *Sources:* 0152 §3.4.
- [ ] **Full collateral carries a resolved lock.** From DAA 750 a resolved claim's lock is carried by
  the whole collateral, with a reserve of four floors for the accuser. *Fence:*
  `palw_final_lock_full_collateral` (V02 option a). *Code:* `final_lock_full_collateral_active_at`.
- [ ] **F + 1,000.** From DAA 750 a resolved `Valid` seat's lock lives until `F + 1,000`, and it stops
  being slashable once it stops being committed. This applies only to claims licensed at or after 750.
  Earlier licences keep licence + 3,000. *Fence:* `palw_final_lock_life`. *Code:*
  `final_lock_life_at`, `final_lock_life_active_at`.
- [ ] **The retroactive re-date.** At the block that crosses DAA 1,300, every `Valid` seat lock of a
  `Final` claim is re-dated to `min(expiry, max(F + 1,000, H))`. After it, a `Final` claim dates its
  seat locks at `F + 1,000`, shorter or not. The producer's E recovery (F + 3,000) and the court's
  liability records do not change. Locks of claims with DA records are excluded. *Fence:*
  `palw_final_lock_life_retro` (requires `palw_final_lock_life` below it). *Code:*
  `palw_final_lock_life_retro_crossing_v1`, `palw_lock_final_daa_retro_v1`,
  `palw_final_lock_retro_expiry_v1`, `palw_bond_resolved_locks_v1`.

## 10.4 One ledger of committed collateral (A)

- [ ] A-1…A-5, and **A-6, one invariant at every gate: `committed + accuser ≤ C`**. *Sources:* 0152 §3.5.
  *Code:* `core/palw_economic_safety_v1.rs`, fence `palw_economic_safety`.

## 10.5 Slashing and the reporter reward (S)

- [ ] The action-based slash schedule, the conviction funnel, the rate `min(25 % · C₀, 3 · G_fp)`
  (S-4), the minimum slash as a share of escrow, and the reporter's reward. Say what is burned and
  what is paid. *Sources:* 0152 §3.6. *Code:* `core/palw_slash.rs`, `min_slash_permille_of_escrow`,
  `palw_reporter_payout_key_v1`.
- [ ] Which offences (chapter 09 §9.4) map to which schedule rows.

## 10.6 Vesting (V)

- [ ] V-1 layout, V-2 what vests (and V-2b, the execution-lane rights of a convicted Final), V-3
  deltas, carriage, root and consistency, V-4 maturity, the fold order and Mainnet Decision A, V-5 burn
  on conviction (keyed on the row), V-6 no escape, V-7 the mint budget, V-8 the escape closed for rows.
  *Sources:* 0152 §3.3. *Code:* `core/palw_vesting_v1.rs` `PalwVestingRowV1`,
  `core/palw_vesting_read_v1.rs`, `palw_vesting_payout_key_v1`.

## 10.7 Throughput caps (T) and withholding (W)

- [ ] T-1 cadence, T-3 the unminted-reward ceiling, T-4 the 2M weight. *Sources:* 0152 §3.7.
- [ ] Withholding. *Sources:* 0152 §3.8 (audit requirement 7).

## 10.8 Emission

- [ ] The premine cap: genesis mints one number, and everything else is a carve. *Sources:* 0059, 0061
  (the testnet-11 values).
- [ ] The block subsidy (testnet-12 genesis: `PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI`) and its schedule,
  and issuance per unit of work. *Sources:* 0137 §8. *Code:* `core/palw_reward_v2.rs`.
- [ ] The carves at Final: worker (vested), panel (0124), the validator carve of one fifth (0126,
  bounded by `palw_validator_payout_bounds`), and the 5 % that buys the line's pair (0091).
  *Code:* `core/palw_economic_payout_v1.rs`, `core/palw_panel_economy_v1.rs`.
- [ ] The floor class's minimum share. *Sources:* 0068.

## 10.9 Liveness is structural; collateral covers fraud

- [ ] D1–D6: which collateral requirement is retired (`window_bind × dearest claim`) and where the
  structural guarantee replaces it. *Sources:* 0151. *Code:* `core/palw_genesis_v2.rs`
  (`BondCannotSustainBindWindow`).

## 10.10 Claim capacity (ADR-0160, dormant)

- [ ] The four-way split of a bond (liability, issuance, fork-choice cap, network share), ρ as a risk
  tier with a breaker that can only lower it, aggregate liability, and the audit door. **Every rule is
  behind t12-only fences that are dormant until a common height `H_cap`.** This section is written
  when ADR-0160 lands. Until then it links the branch. *Sources:* 0160 §3–§10.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis (R-core+: B, SR, V, L, A, S, T, W). DAA 750: `palw_bond_maturity_early`, `palw_final_lock_full_collateral`, `palw_final_lock_life`. DAA 1,000: `palw_bond_maturity`. DAA 1,300: `palw_final_lock_life_retro`. ADR-0160 fences: dormant |

**Design:** `design/palw/collateral.md` and `design/palw/claim-capacity.md`.
