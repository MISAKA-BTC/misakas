# PALW spec — 10. Collateral and economics

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/collateral.md](../../design/palw/collateral.md). The code is the truth, and
> disagreements are listed in [divergences.md](divergences.md). Rule families keep ADR-0152's labels
> (B, SR, L, A, S, R, V, T, W) as sources. Their full as-written text is in
> [design/palw/archive/0152](../../design/palw/archive/0152/README.md).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (every rule below is active from
genesis through `palw_rcore_plus`, except where a fence is named)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P5, P6. Also the design bar: the most an absconder can extract is at most what
the chain can recover.

Symbols used below:

| Symbol | Meaning | Code |
| --- | --- | --- |
| `C` | the bond's posted collateral | `PalwBondStateV2.collateral` |
| `E` | the claim's escrowed reward | `claim_escrow_reservation_v1` |
| `w` | its weight reservation | |
| `rr` | its reserved realizable rights | `rights_reserved` |
| `s` | the buyback bound: 5 % of `E` when the line's pair is open, else 0 | `palw_model_buyback_slice_v1` |
| `G_res` | `w + R + s` | `claim_g_v1` |
| `G` | `E + G_res` | `claim_g_v1` |
| `basis_k` | the recounted number of independent attesters (08 §8.4) | `palw_receipt_set_basis_k_v1` |
| floor | the producer and registration floor, `min_collateral_sompi` | 13,000 MSK on testnet-12 |
| seat floor | 130,000 MSK on testnet-12 | |
| ceiling | `C × 500 ‰` | `fp_max_exposure_ratio_permille` |

## 10.1 The bond (B)

- **PALW-CO-1 (B-1).** A bond's collateral MUST be one account of slashable stake. It is debited on
  the one committed ledger (§10.4) against the ceiling. The uncommitted half funds action tiers,
  accuser exposure and reporter rewards.
- **PALW-CO-2 (B-2).** Registering a bond, and producing, MUST require the floor. Sitting on a panel
  MUST require the seat floor. Past `palw_rcore_plus`, `palw_bond_registration_floor_v1` returns the
  floor. No per-block cap applies to `BondRegistered`.
- **PALW-CO-3 (B-3).** A bond's withdrawal gate (`palw_bond_collateral_is_locked_v6`) MUST hold while
  any of the following is true:
  - its committed amount net of registration exposure is > 0;
  - its accuser exposure is > 0;
  - it is payee of a vesting row not yet mature by lock predicate V-4(a).

  The gate MUST NOT hold merely because the bond is in the table, the chain is in a licence halt, a DA
  session is open on the row, or a row is carried. The exit bound is
  `max(since + withdrawal delay incl. DA lattice, last Final + 9,000)`. On testnet-12 that delay is
  12,900 (7,500 + 5,400), given by `palw_v2_bond_withdrawal_delay_at_v1(bundle, palw_da_court, 0)`.
- **PALW-CO-4 (B-4).** No `Ejected` status and no operator tombstone exist. A bond MAY produce only
  while its current posted collateral (after slashes) is at least the floor
  (`palw_bond_producer_floor_shortfall_v1`). An attempt from a bond below the floor is skipped with
  the non-fatal `ProducerBelowFloor`; the block stands. The same predicate is the node's and the RPC's
  readiness verdict (`ready_to_produce_v3`). The only way back above the floor is a new bond under a
  new key and operator identity, because the registry is append-only.
- **PALW-CO-5.** One operator identity backs at most one bond, ever (`DuplicateOperator` under
  `palw_operator_id_unique`, armed at testnet-12 genesis). A registration proves possession of its
  operator key.
- **PALW-CO-6 (B-5).** Vesting rows MUST NOT count as collateral.
- **PALW-CO-7 (maturity).** A non-genesis bond MUST NOT be drawn to a panel, or counted as a ready seat,
  until `registration DAA + 1,000` (`PALW_T12_BOND_MATURITY_WINDOW_DAA`). This applies from DAA 750
  (`palw_bond_maturity_early`) and from DAA 1,000 (`palw_bond_maturity`). The window counts from the
  registration DAA even for bonds registered before 750. Genesis bonds are exempt. Maturity is judged
  at the claim's anchor DAA, not at the binding block (`palw_seat_maturity_floor_v1`, shared by the
  assembler and acceptance).
- **PALW-CO-49 (a bond in its first block).** Past `palw_bootstrap_activation` (testnet-12 from genesis),
  a block's own attempt resolves its bond against the parent state's bonds plus any `BondRegistered`
  in the block's own mergeset. A registration is therefore usable by the block that merges it, not only
  by that block's child. A block's own body is never in its own mergeset, so this does not let a
  lone producer bootstrap a stopped chain. The clock lane does that (13 §13.3).

**Code:** `core/palw_state_v2.rs` (`PalwBondStateV2`, `PalwBondStatusV2`,
`palw_bond_collateral_is_locked_v6`, `palw_bond_producer_floor_shortfall_v1`);
`core/palw_producer_v2.rs` (`ready_to_produce_v3`); `config/params.rs` (`PalwBondMaturityV1`).

## 10.2 The staged reservation (SR)

- **PALW-CO-8 (SR-1).** A claim's commitment on its producer's bond MUST be
  `palw_claim_commitment_v1(params, claim, now_daa)`:

  | Phase | Commitment |
  | --- | --- |
  | `Provisional`, `PanelBound` | `w + E + rr` |
  | `ReceiptLicensed`, escrow released | `w + rr` |
  | `ReceiptLicensed`, escrow held | `w + E + rr` |
  | `Voided{BindTimeout}`, free-prompt claim on its abandon hold (`fp_abandon_hold_daa`, 600 on testnet-12) | `w + E + rr` (`E` is 0 on the FP lane) |
  | `Final`, any other void, retired | 0 |

- **PALW-CO-9 (SR-1 release).** `escrow_released` MUST be set at licence only if all four hold:
  - the recorded door is Quorum or Coverage, and the recounted `basis_k ≥ 2`;
  - every panel seat carried a `Valid` receipt, and no `Unavailable`, `Incapable`, `Sampled` or
    missing receipt is present;
  - the claim was never redrawn;
  - the class is not in C7.
- **PALW-CO-10 (SR-1b).** A supplementary receipt set that lands by
  `min(licensed_daa + ⌊window_challenge_at/2⌋, bound_daa + window_receipt)` and makes the four
  conditions true MUST flip `escrow_released` in that block (`palw_rcore_supplementary_flips_v1`). A
  flip never happens later and never reverses.
- **PALW-CO-11 (SR-2).** At licence the claim MUST record `licence_door`, `basis_k` and
  `escrow_released` (`claim.rcore`), and they are copied into the liability record and the vesting row.
  The door name never sets `basis_k`.
- **PALW-CO-12 (SR-3, SR-4).** Every write of a claim's phase or R-core fields MUST move the bond's
  reservation by `commitment(before) − commitment(after)`, in one funnel (`move_commitment`). A
  licensed claim never re-reserves.
- **PALW-CO-13 (SR-6).** A receipt set whose `Valid`s include an unbacked signer MUST license on the
  backed subset if that subset still meets the door. Unbacked `Valid`s get no lock, no credit and no
  liability.
- **PALW-CO-14 (SR-7).** Every work gate MUST check `committed + commitment(new) ≤ ceiling` on live
  state at the gate's own DAA and escaped depth: attempt admission, the FP ceilings and the draw. It
  also checks A-6's invariant (§10.4).
- **PALW-CO-15 (C7).** C7 is the set of classes whose `verification_window_spans ≥ 1,000`
  (`PALW_RCORE_C7_WINDOW_SPANS_V1`), united with `palw_rcore_conservative_classes` (testnet-12: the 2M
  class id). A C7 claim holds `w + E + rr` to Final, owes its replay until Final, and is capped at its
  class's `max_inflight_claims`.

Charging on voids is stated with the void reasons in [07](07-claim-lifecycle.md) §7.7 (SR-5, SR-9).

**Code:** `core/palw_state_v2.rs` (`palw_claim_commitment_v1`, `claim_escrow_reservation_v1`,
`palw_rcore_release_due_v1`, `palw_rcore_supplementary_flips_v1`, `move_commitment`).

## 10.3 Seat locks (L), and how long they live

- **PALW-CO-16 (L-1).** Each counted `Valid` signer MUST lock `palw_rcore_lock_v1`. With vesting rows
  landed (`PALW_RCORE_VESTING_ROWS_LANDED_V1 = true`) that is `palw_rcore_lock_vested_at_cap_v1`:
  `palw_seat_lock_required_v2(G_res − s + s_cap, k') + ⌈100 ‰ · (E − s_cap) / k'⌉`, where
  `k' = max(basis_k, 2)` and `s_cap = 5 % · E`. A node MUST refuse to start a network that arms
  `palw_rcore_plus` from a build without the rows (`palw_rcore_build_can_run_v1`).
- **PALW-CO-17 (L-3).** A lock is written at licence, or at a supplementary set, with the signer's
  attested mask (a V1/V2 `Valid` records the full mask, a V3 `Valid` its own). A lock never decreases
  before Final and is not pruned while its row exists. When DA re-keys a row, the claim's live locks
  are re-dated to the row's new expiry in the same funnel.
- **PALW-CO-18 (lock life).** Every lock of a resolved claim is dated at Final.
  - **Below DAA 750:** `F + window_court` (3,000 on testnet-12), bounded by the second clock.
  - **From DAA 750** (`palw_final_lock_life`, keyed on the dating block's DAA): a `Valid` seat's lock
    is dated `F + PALW_FINAL_LOCK_LIFE_DAA_V1` (1,000), never longer than `window_court`. Past that
    height a lock is slashable only while it is committed (`palw_false_valid_lock_slashable_v1`).
    Only the lock moves: the liability record, the vesting row's clawback of `E`, and the court and DA
    windows keep `window_court`.
  - **At the block crossing DAA 1,300** (`palw_final_lock_life_retro`): every seat lock of an honest
    `Final` claim is re-dated once to `min(expiry, max(F + 1,000, H))`. From then on, a `Final` at
    `F ≥ H` dates its seat locks at exactly `F + 1,000`. Locks of claims with DA history, voided
    claims, reversed Finals and claims not yet Final are untouched.
- **PALW-CO-19 (L-4).** At bind each seat's duty MUST be
  `duty_bind = min(max(λ-term, lock_2), ⌊commitment_at_bind / seat_count⌋)` (`palw_rcore_duty_bind_v1`),
  so `seat_count × duty_bind ≤` the claim's forfeit. The committed ledger counts `max(duty, live lock)`
  per (seat, claim). The duty is released at Final and the lock remains.
- **PALW-CO-20 (L-4b).** A seat is eligible for a claim only if
  `committed + max(duty_bind, lock_2) ≤` its room (`palw_rcore_seat_eligibility_v1`). A lock that
  exceeds the duty (2M, FP) is topped up from headroom at licence.

**Code:** `core/palw_state_v2.rs` (`palw_rcore_lock_v1`, `palw_rcore_duty_bind_v1`,
`palw_rcore_seat_eligibility_v1`, `final_lock_life_at`, `palw_final_lock_life_retro_crossing_v1`,
`palw_lock_final_daa_retro_v1`, `palw_final_lock_retro_expiry_v1`, `palw_bond_resolved_locks_v1`);
`core/palw_economic_safety_v1.rs` (`palw_seat_lock_required_v2`).

## 10.4 One ledger of committed collateral (A)

- **PALW-CO-21 (A-1, A-2).** A bond's committed amount MUST be `palw_bond_committed_v1`: its own claims'
  commitments, plus registration exposure, plus the sum of `max(duty, live lock)` over its
  (seat, claim) pairs. It is the only reader of a bond's commitment.
- **PALW-CO-22 (A-6, one invariant).** At every gate, `committed + accuser ≤ C`. One function computes
  both gates (`palw_rcore_gate_room_of_v1`):
  - **Work** (a claim, a duty, a lock top-up, an FP commitment): room
    `min(ceiling − committed, C − committed − accuser)`.
  - **Accuser** (a court, a held dissection, a DA accusation): room
    `C − max(committed, ceiling) − accuser`.

  Accuser exposure is the sum of open DA sessions, refuted exposure held per claim, and court
  challenger reservations (`palw_accuser_exposure_v1`).
- **PALW-CO-23 (resolved locks, from DAA 750).** Past `palw_final_lock_full_collateral`, the locks a
  bond holds on claims that are no longer live count against the whole `C`, not the ceiling. The work
  room becomes `max(launch, min(ceiling − (committed − resolved), C − R − committed − accuser))`, with
  `R = 4 × floor` held back for accusers (52,000 MSK on testnet-12). The draw's filter, admission and
  the producer's facts read the same room (`palw_rcore_gate_room_split_of_v1`,
  `palw_bond_accuser_reserve_v1`).
- **PALW-CO-24 (A-4, A-5).** A duty is not slashable, because `Withheld` abstains. The first 50 % of any
  action slash is collectible from uncommitted stake. `slash_bond` saturates, and what it actually took
  is the conviction's collected debit.

**Code:** `core/palw_state_v2.rs` (`palw_bond_committed_v1`, `palw_rcore_gate_room_of_v1`,
`palw_bond_headroom_v1`, `palw_accuser_room_v1`, `slash_bond`).

## 10.5 The action-based slash schedule (S) and the reporter reward (R)

- **PALW-CO-25 (S).** Every slash goes through `slash_bond`. What is not paid as reward is burned. Action
  tiers are capped at `3 × G`, where `G` is the claim's own, fixed at the fraud (`claim_g_v1` reads the
  liability row's recorded values where one exists). The tiers:

  | Tier | When | Charged |
  | --- | --- | --- |
  | S0 | capacity voids (`BindTimeout`, `NoCapablePanel`); a first failed panel | nothing |
  | S0′ | a second failed panel (second `ReceiptTimeout`, `UnavailableQuorum`, `NotReplayBacked`); always for C7 | the commitment at its stage. No strike, no record, no reward |
  | S1 | DA-confirmed `ProducerWithholding` | the commitment (+ `E` from uncommitted stake if already released); a strike. From the 3rd strike within 7,500 DAA, add `min(10 %·C, 3G)` |
  | S2 | invalid caught before Final: `CourtFraud`, `ExecutorRefuted`, a `PanelFalseValidV2` that acts on the claim, a court default | the commitment (+ `E` if released) + `min(10 %·C, 3G)` |
  | S3 | fraud whose claim has an unmatured vesting row | the whole row burned + producer `min(25 %·C, 3G)`; once per claim |
  | S3-FP | an FP claim convicted after Final | executor `min(25 %·C₀, 3·G_fp)` |
  | S4 | a liable false `Valid` signer; a covering signer on a DA default | its lock (taken first) + `min(25 %·C, 3G)`, only while it holds a live lock; once per (seat, claim) |
  | Eq | `ExecutorEquivocation` (standalone) | `min(C, 3·G_eq)` on testnet-12 (`palw_eq_cap_basis_v1`) |

- **PALW-CO-26 (the funnel).** A conviction MUST open (recording `C₀` and whether the bond's exit gate
  was shut), run its legs, and close by writing its consumed record last, with `amount` (nominal) and
  `collected` (debits from bonds whose gate was shut). S0′ is not a conviction. *Code:*
  `open_conviction_v1`, `close_conviction_v1`.
- **PALW-CO-27 (R-1, R-2).** `reward = ⌊r × max(0, collected − X)⌋` with `r` = 1,000 bps.
  - `X = min(lock, G_res / basis_k)` per load-bearing signer of a claim that reached Final.
  - A bond whose gate was open contributes 0 to `collected`.
  - Burned vesting and S0′ forfeits are never in the base.
  - A DA default's base is the producer's collected debit only.
- **PALW-CO-28 (R-3, R-4).** A reward for a kind 0, 3 or 4 conviction goes by commit–reveal:
  - The reporter first commits `ReporterCommitted` (tag 53) over the offence key and the consumed
    `evidence_id`. At most 64 open commitments per bond; pruned after `window_court` unless protecting
    an open reveal.
  - The reporter then reveals with `ReporterRevealed` (tag 54). The earliest matching commitment
    strictly before the consuming block wins, and `reporter ≠ accused`.
  - The reveal window is 600 DAA (`window_receipt`). The reward is written at the first step-2 sweep
    past it.
  - A DA default pays the earliest defaulted session's accuser. A proven court verdict pays its
    challenger. A court default opens nothing.

**Code:** `core/palw_slash.rs`, `core/palw_state_v2.rs` (`palw_rcore_s1s2_action_v1`,
`palw_rcore_s3s4_action_v1`, `palw_rcore_strike_v1`, `palw_rcore_eq_cap_v1`, `open_reporter_reward`,
`palw_reporter_payout_key_v1`).

## 10.6 Vesting (V)

- **PALW-CO-29 (V-1, V-2).** At Final, `finalize_claim` MUST write a vesting row
  (`PalwVestingRowV1`) instead of paying out:
  - What vests: the producer's split, each credited seat's share, and the reserve.
  - What does not: the ADR-0091 buyback slice, the work-price remainder, FP receipt-spend payouts, and
    execution-lane rights.
  - The row copies `job_identity`, `free_prompt`, `trace_root`, `segment_count`, `licence_door` and
    `basis_k`.
- **PALW-CO-30 (V-4).** A row matures only when all three hold, and maturity is latched:
  - the lock predicate over its `(expiry_daa, settled_at_final)` has run out;
  - the chain is not in a licence halt (`palw_chain_vesting_halted_v1`);
  - no DA session is open on its claim.
- **PALW-CO-31 (V-5, V-6).** A conviction binding the claim (S3, a post-Final DA default, a post-Final
  court conviction) MUST burn the whole row (`burn_vesting_row`). The burned value is never minted. A
  row is not a UTXO and cannot be moved, spent or pledged.
- **PALW-CO-32 (V-7).** At step 3d each block moves reporter rewards first, then mature rows in
  `(expiry_daa, claim_id)` order, never skipping.
  - The budget is `8 − min(2, market rows waiting)` new queue keys (`palw_vesting_mint_plan_v1`).
  - Producer legs are keyed `palw_vesting_payout_key_v1(claim_id)`, and reporter legs
    `palw_reporter_payout_key_v1(offence_key)`.
- **PALW-CO-33 (V-8).** Only attempt licences with `basis_k ≥ 2` settle the anchor clock that rows
  mature on. FP licences and un-upgraded S2 licences do not.
- **PALW-CO-34 (V-2b).** Every conviction forfeits the claim's unminted execution-lane rights:
  - **by root** for contradictions 5, 6, 8, 11 and 12;
  - **by claim** for 9, 10, 13, DA defaults and court convictions.

**Code:** `core/palw_vesting_v1.rs` (`PalwVestingRowV1`, `palw_chain_vesting_halted_v1`,
`palw_vesting_mint_plan_v1`, `palw_vesting_payout_key_v1`), `core/palw_vesting_read_v1.rs`.

## 10.7 Throughput caps (T) and withholding (W)

- **PALW-CO-35 (T-1).** Throughput is bounded by the cadence:
  - one own attempt per chain block, plus merged blues;
  - per-class epoch budgets (05 §5.4);
  - the per-span admission cap.
- **PALW-CO-36 (T-2).** A non-base class is admitted only within its panel room, computed as follows:
  - the room is `palw_panel_capacity_by_rate_v1` minus what is owed;
  - capacity counts effective ready operators, `ready_eff` (`palw_panel_ready_eff_v1`, SW-9);
  - a licence of a non-C7 class with `basis_k ≥ 2` frees its replay;
  - a C7 class owes every claim until Final and is capped at `max_inflight_claims`;
  - no bond holds more than `⌈c_class/2⌉` unlicensed claims of one non-base class.
- **PALW-CO-37 (T-3).** Unlicensed escrow per bond stays within its ceiling. Network-wide live vesting
  is bounded by recent Finals times E (`vesting_created_sompi` counters).
- **PALW-CO-38 (W).** Withholding is charged only when DA-confirmed (S1), or by S0′ when it sinks both
  panels.

## 10.8 Emission

- **PALW-CO-39 (premine cap).** Genesis MUST mint one number, the premine, capped at 10,000,000,000 MSK.
  Every other allocation — genesis bonds, community rows, operator wallets — is a carve of it, never
  an addition.
- **PALW-CO-40 (subsidy).** Each chain block's subsidy follows the network's schedule. On testnet-12 the
  genesis block subsidy is 444,562,014,000 sompi (4,445.62 MSK at 10^8 sompi per MSK;
  `PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI`). PALW reward MUST be a carve of that subsidy, never an
  addition to it.
- **PALW-CO-41 (the carve).** Past `palw_overlay_carve` (testnet-12 from genesis, `{2,000 bps, 720 ‰}`):
  - the DNS validator pool is 20 % of the subsidy;
  - a claim escrows `⌊subsidy × 720 / 1000⌋` of the block that carried its attempt (3,200.85 MSK at
    genesis), and the coinbase withholds exactly that. Both are resolved at the lower of the carrying
    block's and the paying block's DAA (`palw_overlay_escrow_carve_at_v1`).
- **PALW-CO-42 (payout at Final).** A `Final` claim's escrow is paid as follows:
  - **Model-class claims past `palw_economic_payout`** (testnet-12 from genesis) are paid
    `min(escrow, attempted_ccu × rate)`. On testnet-12 the rate is 900,000,000 sompi per giga
    MAC-eq, and the panel's share is `clamp(α·C_V / (C_P + α·C_V), 100 ‰, 300 ‰)` with α = 100 ‰
    (08 PALW-VF-37). The claim snapshots its economics at acceptance. What the price leaves is never
    minted.
  - **The floor class** stays on the fixed split: 80 % to the producer and 20 % to the panel pool.
  - **The buyback slice** (5 % of `E`) buys from the line's pair where one is open (15 §15.2).
  - **Legs that vest** are the worker, seat and reserve legs (§10.6).
- **PALW-CO-43 (the floor's minimum).** The floor class keeps its minimum cadence as the liveness floor.
  Model classes carry the economy (ADR-0068, as superseded for shares by ADR-0137: a share is a
  result, 05 §5.4).

**Sources:** ADR-0059, ADR-0042 D10, ADR-0126 D1–D3 and §6a, ADR-0132 (Upgrade C), ADR-0124, ADR-0091,
ADR-0068. **Code:** `config/params.rs` (`palw_overlay_carve`, `palw_economic_payout`),
`core/palw_economic_payout_v1.rs`, `core/palw_reward_v2.rs`, `cons/processes/coinbase.rs`.

## 10.9 Liveness is structural; collateral covers fraud

- **PALW-CO-44 (ADR-0151 D3).** The clock MUST NOT depend on any bond's collateral. The heartbeat lane
  is armed from genesis, and no `bits`-priced lane is producible, so the DAA advances every heartbeat
  interval whatever any bond can afford (06 §6.4, 13 §13.3).
- **PALW-CO-45 (D2).** The genesis gate MUST NOT require bonds to sustain `window_bind × dearest claim`
  when the clock advances without a claim (`verify_palw_genesis_v2_with_clock_v1`). The other genesis
  checks still run: a bond declares only what its outpoint holds, panels can be seated, and the
  catalogue agrees.
- **PALW-CO-46 (D1).** Genesis collateral is sized to reachable fraud liability: concurrency per
  class, with liability per claim equal to `palw_max_fraud_gain_v1` = escrow + the fork weight a Valid
  Final authorizes (`palw_v2_collateral_for_class_set_v1`). *The runtime half, the producer's live
  reservation, is open.*
- **PALW-CO-47 (D4).** Admission capacity (`reserved_exposure`, released at Final and at void alike) and
  slash liability (`slashable_locks`) MUST be separate ledgers. On testnet-12 the liability ledger is
  armed from DAA 0.
- **PALW-CO-48 (D5).** Duration MUST NOT be weight. The payout has no term in windows or DAA, and a
  claim's deadline is its class's own (08 PALW-VF-29).

**Sources:** ADR-0151 D1–D6. **Code:** `core/palw_genesis_v2.rs` (`verify_palw_genesis_v2_with_clock_v1`,
`palw_clock_advances_without_a_claim_v1`, `palw_v2_collateral_for_class_set_v1`), `core/palw_economic_payout_v1.rs`.

## 10.10 Claim capacity (ADR-0160)

**Placeholder.** ADR-0160 v3 is imported after the int-6 integration, in its final form. Its fences
are dormant until `H_cap`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | R-core+ (B, SR, L, A, S, R, V, T, W) from genesis (`palw_rcore_plus`, DAA 0); `palw_overlay_carve` and `palw_economic_payout` from genesis. DAA 750: `palw_bond_maturity_early`, `palw_final_lock_full_collateral`, `palw_final_lock_life`. DAA 1,000: `palw_bond_maturity`. DAA 1,300: `palw_final_lock_life_retro`. X10 (`palw_rcore_attributed_charging`) is not declared |

**Sources:** ADR-0152 §3.1–§3.8, §2 (archive 0152/02–04); ADR-0154, ADR-0155; ADR-0151; ADR-0065 D1, D4–D5; ADR-0064; ADR-0059; ADR-0061 (testnet-11's collateral); ADR-0126; ADR-0132.
