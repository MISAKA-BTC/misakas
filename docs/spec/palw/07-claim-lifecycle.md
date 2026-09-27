# PALW spec — 07. Claim lifecycle

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. Most of this
> chapter's sources are in ADR-0152 (R-core+ v3.1), which is not on this branch (see
> [adr/INVENTORY.md](../../adr/INVENTORY.md)), and in the post-launch fences.

**Purpose.** This chapter is the settle-later half of chapter 01 §1.3. An eligible inference becomes a
**claim**. The claim is anchored and bound to a panel, licensed on receipts, left open through a
challenge window, and then becomes `Final` and is paid, or it is voided. This chapter defines the
states, every transition and deadline, every void reason and what each void costs, the retries that
replace a void, and the escrow and payout the states drive. Chapter 08 (the panel), chapter 09 (the
court) and chapter 10 (reservations, locks and vesting) specify the machinery the transitions call.

**Principles served:** P6 (only eligible work becomes a claim), P3 and §3 "not paid twice", and
chapter 01 §1.3.

## 7.1 States

- [ ] The phases `Provisional → PanelBound → ReceiptLicensed → Final`, with `Voided` and
  `DefaultDisputed`, and the sources `Attempt` and `FreePrompt { quanta, spent }`. For each phase:
  what it records, which objects move it, and which deadline it waits on. *Sources:* 0042 D2, 0037
  D2–D9 (the job state machine, carried), 0152. *Code:* `core/palw_state_v2.rs` `PalwClaimStateV2`,
  `PalwClaimPhaseV2`, `PalwClaimSourceV2`; `core/palw_job_state.rs`.
- [ ] The job identity carried by a claim (`job_identity`). *Sources:* 0152 J-1. *Code:*
  `core/palw_job_identity.rs`.

## 7.2 From eligible attempt to claim

- [ ] What makes an eligible attempt, or a spent free-prompt quantum, into a claim, and what the
  claim fixes at creation (class, work vector, producer bond, escrow). *Sources:* 0074, 0044 D5, 0148.
- [ ] Admission against live state: class admission, the producer bond's exposure and the staged
  reservation (chapter 10). *Sources:* 0152 SR-7, 0042 D6. *Code:* `core/palw_admission_v2.rs`,
  `core/palw_exposure.rs`.

## 7.3 Anchor and bind

- [ ] **One state, bind only in the anchor block.** A claim binds its panel in the anchor block and
  nowhere else. The anchor block is an attempt block. The grinding surface this leaves is stated.
  *Sources:* 0152 SW-8 and IA-1c, and its correction in
  [t12-panel-seed-2026-09-25.md](../../t12-panel-seed-2026-09-25.md). *Code:*
  `palw_rcore_duty_bind_v1`, `palw_rcore_bind_prices_v1`, `palw_provisional_bind_deadline_v1`.
- [ ] The panel seed is the anchor attempt's execution commitment, not its identifier. Swapping a
  signature, timestamp or nonce cannot redraw the panel. *Fence:* `palw_panel_seed_execution`
  (DAA 750). Chapter 08 §8.1.
- [ ] **Operator anchor (stopgap).** From DAA 750, only attempts of the genesis operator bonds anchor
  panels. During that period the draw trusts the operators, and the Spec says so. *Fence:*
  `palw_operator_anchor` (DAA 750). *Sources:* the operator's decision of 2026-09-26, the launch
  note §00. *Code:* `core/palw_operator_anchor_v1.rs`.
- [ ] **Anchor at the exposure ceiling.** An operator's own attempt at its bond's exposure ceiling
  still anchors the claims due at it, and does not itself become a claim. *Fence:*
  `palw_anchor_at_ceiling` (DAA 750, at or above `palw_operator_anchor`). *Code:*
  `core/palw_producer_v2.rs`, `core/palw_admission_v2.rs`.
- [ ] `BindTimeout` is applied in the anchor block, at that block's DAA. It releases the producer's
  reservation. *Sources:* 0152 IA-1c, DL-1.

## 7.4 Refused draws: void or retry

- [ ] **Eligibility refusal (SW-10).** The draw is refused when the eligible stake is below the floor:
  `(E + X) · 1000 < (B + X) · 875`, where X is the executor's capped SW-2 weight. It is also refused
  when, after load exclusion, fewer eligible operators remain than there are seats. Chapter 08 §8.2.
  *Sources:* 0152 SW-10 and IA-1b.
  - Before DAA 1,300 the claim is voided at the anchor slot. Its escrowed worker reward is burned and
    no collateral is taken.
  - **From DAA 1,300 (`palw_floor_refusal_retry`)** the claim **re-anchors at the next slot instead of
    voiding**. Refusal is judged on the anchor block's state before its objects are applied, so the
    anchor producer cannot redraw a draw it dislikes. The existing deadlines still apply. *Code:*
    `palw_floor_retry_is_due_v1`, `floor_refusal_retry_active_at`.
- [ ] **No capable panel.** From DAA 750 (`palw_registry_resilience`), a claim waiting to bind is not
  voided at once when ready seats cannot fill the panel at the anchor. It is retried until its bind
  deadline, and then voided `NoCapablePanel`. *Code:* `palw_claim_awaits_ncp_retry_v1`,
  `palw_ncp_retry_is_due_v1`.

## 7.5 Receipts, licence and the challenge window

- [ ] Receipts are evidence, not votes (chapter 08). A claim is licensed on the backed subset of its
  panel, once coverage is reached, optimistically or by coverage. *Sources:* 0098, 0108, 0152 SR-6.
  *Code:* `palw_select_optimistic_licence_v2`, `palw_select_coverage_licence_v2`,
  `palw_v2_object_licenses_claim_v1`, `palw_v2_licence_backed_seats_v1`.
- [ ] Licence order: which claims a block may license first. The known issue V04 (licences stuck
  behind claim-id order) is stated as the current rule until it is fixed. *Code:*
  `palw_licence_candidate_order_v2`, `palw_licence_offer_order_v1`; node side
  `kaspad/src/palw_licence_order.rs`.
- [ ] A replay-backed licence, where a class requires one. *Sources:* 0133, 0152. *Code:*
  `palw_rcore_licence_awaits_replay_v1`.
- [ ] The challenge window: its length, the short-window variant, and what may still void a licensed
  claim (a court or a DA default). *Sources:* 0042, 0133 (verification on its own clock).
  *Code:* `set_palw_short_challenge_window`, `palw_claim_final_floor_v1`,
  `palw_panel_holds_to_final_v1`.

## 7.6 Final, settlement and payout

- [ ] `Final`: when a claim reaches it, and what becomes irreversible. *Code:* `final_work_iter`,
  `settled_attempt_finals`.
- [ ] **PALW settles on its own anchors.** A payment is settled at the settled-anchor depth
  (testnet-12: 30). DNS finality is not required. *Sources:* 0127, 0129. *Code:*
  `core/palw_settlement_v1.rs` `palw_settlement_v1`, `palw_settled_anchor_depth_v1`,
  `palw_settled_anchor_floor_daa_v1`. The "finality depth 600 blue" of the launch note is derived from
  the challenge window. Write down how.
- [ ] Payout at Final: the worker reward (vested, chapter 10 V), the panel's share (0124), the
  validator carve (0126) and the 5 % that buys the line's pair (0091, chapter 15). *Code:*
  `pending_payout`, `pending_payouts_iter`, `palw_panel_payout_key_v1`,
  `palw_vesting_payout_key_v1`, `core/palw_economic_payout_v1.rs`.

## 7.7 Void reasons and what each costs

- [ ] One row per `PalwVoidReasonV2`: `BindTimeout`, `ReceiptTimeout`, `CourtFraud`,
  `ProducerWithholding`, `NoCapablePanel`, `UnavailableQuorum`, `NotReplayBacked`, `CourtDefault`,
  `CourtHeldVerdict`. For each: the trigger, the DAA at which it applies, what the escrow does (burn
  or refund), what the producer forfeits, and which offences it records (chapter 09). *Sources:*
  0152 SR-5, SR-8 and SR-9, 0065 D4 (Unavailable is an abstention). *Code:*
  `palw_court_verdict_void_reason_v1`, `palw_court_default_void_reason_v1`,
  `palw_void_binds_claim_v1`, `palw_escrow_destroyed_by_delta_v2`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis, plus DAA 750 (`palw_panel_seed_execution`, `palw_operator_anchor`, `palw_anchor_at_ceiling`, `palw_registry_resilience`) and DAA 1,300 (`palw_floor_refusal_retry`) |

**Design:** `design/palw/lifecycle.md`.
