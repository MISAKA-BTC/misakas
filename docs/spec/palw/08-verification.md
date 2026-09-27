# PALW spec — 08. Verification

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. The
> stake-weighted draw (SW-1…SW-10), quorum counting (Q-1…Q-7) and the redesigned DA court (DA-1…DA-9)
> are in ADR-0152, which is not on this branch.

**Purpose.** "The inference actually ran, as committed" (chapter 01 §1.2) is checked by
re-execution. For each claim, a **panel** of bonded seats is drawn from operators other than the
executor. Each seat must prove it holds the class's artifact and can run it. Each files a signed
**receipt**, which is evidence and not a vote. Receipts are counted into a licence, and replay checks
a class over spans on its own clock. This chapter defines the draw, seat readiness, receipts, quorum
counting, replay, and data availability. A seat that finds a lie takes it to the court (chapter 09).

**Principles served:** §3 "ran as committed", P7 (independence is drawn, not declared).

## 8.1 The panel seed

- [ ] The draw seed is derived from the anchor attempt's **execution commitment**, so a producer
  cannot regrind it by re-signing. *Fence:* `palw_panel_seed_execution` (testnet-12, DAA 750).
  *Sources:* the CRITICAL finding of 2026-09-25, [t12-panel-seed-2026-09-25.md](../../t12-panel-seed-2026-09-25.md),
  0152 SW-8. *Code:* `core/palw_panel_v2.rs` `palw_panel_draw_seed_v1`,
  `core/palw_verification_v2.rs`, `core/palw_layer_sample_v3.rs`.

## 8.2 The stake-weighted draw (SW)

- [ ] SW-1 scope and fence. SW-2 the weight: the posted collateral of the operator's one bond, in whole
  MSK, capped at `PALW_DRAW_WEIGHT_CAP_MSK_V1` = 1,000,000. SW-3 the key `L_i / W_i` from an integer
  −log2, and the sample. SW-4 genesis seats and floors. SW-5 the outsider, and the jury that stays
  0147's. SW-6 what does not become weighted. SW-7 Sybil resistance. SW-8 is in chapter 07 §7.3. SW-9
  throughput and liveness when stake concentrates. SW-10 the eligible-stake floor (875 ‰), counting
  the executor's weight on both sides. *Sources:* 0152 §3.14, IA-1/IA-1b. It reverses the
  one-ticket-per-operator lottery of 0124 D5 and 0130 on testnet-12. *Code:*
  `PalwPanelDrawPolicyV1`, `palw_panel_stake_ticket_v1`, `palw_panel_stake_outsider_ticket_v1`,
  `palw_panel_stake_entries_v1`, `palw_panel_stake_weight_v1`, `palw_panel_stake_floor_v1`,
  `palw_draw_neg_log2_q64_v1`, `palw_draw_operator_weight_msk_v1`.
- [ ] What happens when a draw is refused: chapter 07 §7.4. That covers the void before DAA 1,300 and
  the retry from 1,300.
- [ ] Operator identity: one operator id per bond (the audit fence `palw_operator_id_unique`).
- [ ] Panel size and the λ of BPS 1 (testnet-12: λ = 5). *Sources:* 0130, 0152. Values in chapter 16.

## 8.3 Seats and readiness

- [ ] A seat is counted only if it is **ready**: the artifact root matches, its chunks are held, it
  is participating, it has collateral for `readiness_collateral_multiple` exposures, and it verified a
  probe within `readiness_probe_max_age_spans`. Readiness V2 is a possession proof over the whole
  artifact. *Sources:* 0133 §11.2, 0152 (readiness horizon 24). *Code:*
  `core/palw_model_registry_v1.rs` (seat readiness), `core/palw_readiness_escalation_v1.rs`.
- [ ] From DAA 750 a bond registered after genesis is outside the draw and the ready count until it
  matures (chapter 10 §10.1). *Fence:* `palw_bond_maturity_early`. *Code:*
  `core/palw_panel_view_v1.rs`.
- [ ] Coverage is a number. A seat holds a shard. A seat may demand the committed leaf it needs to
  judge. *Sources:* 0098, 0099, 0111. *Code:* `core/palw_seat_coverage_v1.rs`,
  `core/palw_shard_panel_v1.rs`, `core/palw_leaf_evidence_v1.rs`.
- [ ] A seat must be able to run the class it judges (capability bound). *Sources:* 0071 D3. *Code:*
  fence `palw_capability_bound`, `PalwCapabilityStateV2`.
- [ ] Routing: execution-class families and model bands. *Sources:* 0034 (as amended by 0053 D5).
  *Code:* `core/palw_routing.rs`.

## 8.4 Receipts and quorum

- [ ] A receipt is evidence, not a vote: what it signs, what it commits, and who may file it.
  *Sources:* 0108, 0026. *Code:* `core/palw_receipt.rs`.
- [ ] Quorum counting: verdicts (Q-1), the doors stay enabled (Q-2), the recount (Q-3), the lock
  price (Q-4), the gate on the first panel (Q-5), whom a conviction binds (Q-6). *Sources:* 0152
  §3.12. *Code:* `palw_seat_uncounted_on_licence_v1`, `palw_rcore_counts_licensed_v1`.
- [ ] Unavailable seats abstain from fork choice. They are not a verdict. *Sources:* 0065 D4.
  *Code:* fence `palw_unavailable_abstains`.

## 8.5 Replay and verification clocks

- [ ] Verification is its own clock. A class verifies over spans, and a starved class stops only
  itself (Verification V2, S1). *Sources:* 0133. *Code:* `core/palw_verification_v2.rs`,
  `core/palw_verification_profile_v1.rs`.
- [ ] Replay refutation and the replay budget horizon. *Sources:* 0133, 0152. *Code:*
  `core/palw_replay_refute_v1.rs`.
- [ ] Sampling schedules re-execution and never convicts. *Sources:* 0028 (windows restated by 0133).

## 8.6 Data availability

- [ ] The DA court, redesigned: scope, state, units (named plus drawn), answers, pause credit,
  re-keying and locks, what a session costs, default, windows and caps. *Sources:* 0152 §3.11
  DA-1…DA-9, amending 0062. *Code:* `core/palw_da_rcore_v1.rs`, `core/palw_panel_da_v1.rs`,
  `core/palw_operator_da_v1.rs`, `core/palw_held_da_v1.rs`.
- [ ] The producer's DA responder is unconditional (a node duty, chapter 14). *Sources:* 0152 IA.

## 8.7 What the panel is paid, and the exposure a seat holds

- [ ] The panel is paid out of the claim's reward, and a seat holds exposure. The amounts are in
  chapter 10. *Sources:* 0124. *Code:* `core/palw_panel_economy_v1.rs`, fence
  `palw_panel_exposure_floor`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis (stake draw, readiness V2, verification V2, DA court). From DAA 750: `palw_panel_seed_execution`, `palw_bond_maturity_early`. `palw_shard_licensing` is dormant (refused by validation) |

**Design:** `design/palw/verification.md`.
