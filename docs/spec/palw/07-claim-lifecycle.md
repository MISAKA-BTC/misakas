# PALW spec — 07. Claim lifecycle

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/lifecycle.md](../../design/palw/lifecycle.md) and
> [design/palw/collateral.md](../../design/palw/collateral.md). The code is the truth, and
> disagreements are listed in [divergences.md](divergences.md).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (from genesis, plus the DAA-750 and
DAA-1,300 fences named below)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P6 (only eligible work becomes a claim); §3, "not paid twice"; chapter 01 §1.3
(execute now, settle later).

An eligible inference becomes a **claim**. A claim is anchored and bound to a panel, licensed on
receipts, and left open through a challenge window. It then becomes `Final` and is paid, or it is
voided. The panel is specified in chapter 08, the court in 09, and reservations, locks and vesting in 10.

## 7.1 States

- **PALW-LC-1.** A claim (`PalwClaimStateV2`) MUST be in exactly one phase (`PalwClaimPhaseV2`):
  - `Provisional`: admitted, not yet bound;
  - `PanelBound`: a panel is bound; waiting for receipts;
  - `ReceiptLicensed`: licensed; the challenge window is running;
  - `Final`: resolved and paid through vesting rows;
  - `Voided { reason }`: resolved without payment;
  - `DefaultDisputed`: the pre-R-core DA state. It is never written past `palw_rcore_plus` (08
    PALW-VF-19).
- **PALW-LC-2.** A claim's source (`PalwClaimSourceV2`) MUST be `Attempt` or `FreePrompt { quanta, spent }`.
  A free-prompt quantum MUST be spent at most once.
- **PALW-LC-3.** A claim records its job identity (`job_identity`): the carrying header's
  `execution_anchor_v3` for an attempt, and `palw_fp_job_pin_v1(commitment)` for a free prompt. The
  value 0 means "not recorded" and never convicts (09 §9.4).
- **PALW-LC-4.** A claim also records its R-core fields `claim.rcore = { licence_door, basis_k,
  escrow_released, served_mask, unserved_seen }` (10 PALW-CO-11). A redraw resets them.

**Code:** `core/palw_state_v2.rs` (`PalwClaimStateV2`, `PalwClaimPhaseV2`, `PalwClaimSourceV2`,
`PalwVoidReasonV2`); `core/palw_job_identity.rs`.

## 7.2 Admission

- **PALW-LC-5.** An attempt that wins its lottery (06), or a spent free-prompt quantum (11), MUST
  become a claim only if its class admits it (03), its producer bond passes the producer floor and the
  work gate (10 PALW-CO-4, PALW-CO-14), and the panel room admits it (10 PALW-CO-36). A refused own
  attempt is skipped, and the block stands (`AttemptExposureCeiling`, `ProducerBelowFloor`,
  `BondClassShareExceeded`).
- **PALW-LC-6.** At admission the claim fixes its class, its work (05), its producer bond, its escrow
  `E` and its commitment (10 PALW-CO-8).

**Code:** `core/palw_admission_v2.rs`, `core/palw_state_v2.rs` (`apply_attempt`), `core/palw_exposure.rs`.

## 7.3 Anchor and bind

- **PALW-LC-7 (slot).** A claim's anchor slot MUST be `bind_base_daa() + anchor_delay` (20 on
  testnet-12). After a redraw, `bind_base_daa()` is the redraw's DAA.
- **PALW-LC-8 (anchor block).** A claim's anchor block is the first block of the selected chain at or
  past its slot that is allowed to anchor (`palw_block_may_anchor_a_panel_v1`).
  - **From genesis:** only attempt blocks may anchor (algo 6 or 9). A heartbeat at the slot anchors
    nothing (SW-8 as integrated).
  - **From DAA 750** (`palw_operator_anchor`): a chain block anchors a claim with slot `s` iff it is,
    or merges, an attempt of a **genesis operator bond** (`PalwOperatorAnchorRuleV1::operator_of_v1`)
    whose DAA is at least `s`. The seed and the draw's clock come from the earliest such operator
    attempt in the DAG (`palw_operator_seed_source_v1`). A block that merges only operator attempts
    older than `s` neither binds nor voids that claim. *Stopgap: panel seeding trusts the operators
    while this fence is armed.*
  - **From DAA 750** (`palw_anchor_at_ceiling`): an operator's own attempt at its bond's exposure
    ceiling still anchors the claims due at it. That attempt itself does not become a claim.
- **PALW-LC-9 (bind only in the anchor block).** A panel MUST bind only in its claim's anchor block,
  on the one state of 08 PALW-VF-8, with the seed of 08 PALW-VF-1. A `PanelBound` carried in any
  other block is refused.
- **PALW-LC-10 (unbound in the anchor block).** At step 4c the anchor block MUST settle every
  `Provisional` claim whose slot it reaches and that it did not bind
  (`palw_void_claims_unbound_in_their_anchor_block_v1`), at the block's own DAA. Such a claim is voided
  `BindTimeout`, or `NoCapablePanel` for a rowed class that cannot seat a panel, unless a retry below
  applies. A void releases the producer's reservation. A free-prompt claim keeps its abandon hold.
- **PALW-LC-11 (backstop).** A claim no anchor block reaches before `accepted_daa + window_bind` (600
  on testnet-12) MUST void `BindTimeout` at that deadline.

**Sources:** ADR-0152 SW-8, IA-1a, IA-1c, DL-1; [t12-panel-seed-2026-09-25.md](../../t12-panel-seed-2026-09-25.md)
(lane A); ADR-0154. **Code:** `core/palw_state_v2.rs` (`palw_void_claims_unbound_in_their_anchor_block_v1`,
`palw_claims_provisional_past_their_anchor_slot_v1`, `palw_provisional_bind_deadline_v1`,
`palw_rcore_duty_bind_v1`); `core/palw_operator_anchor_v1.rs`; the processor
(`palw_chain_block_as_anchor_v1`, `palw_sw8_anchor_delay_for`).

## 7.4 Retries instead of voids

- **PALW-LC-12 (no capable panel, from DAA 750).** Past `palw_registry_resilience`, keyed on the
  anchor block's DAA, the claim is not voided `NoCapablePanel` when its anchor block cannot bind it
  because the class's ready seats cannot fill a panel on the state the draw read. It MUST be re-based
  on that block (`Provisional`, `rebound_daa` = the anchor block's DAA) and retried at its next anchor
  slot. Only the backstop (PALW-LC-11) voids it. (`palw_claim_awaits_ncp_retry_v1`,
  `palw_ncp_retry_is_due_v1`.)
- **PALW-LC-13 (eligibility refusal, from DAA 1,300).** Past `palw_floor_refusal_retry`, keyed on the
  anchor block's DAA, a claim is retried the same way, instead of voided, when its stake draw is refused
  **for eligibility at every seed** on the anchor block's pre-object base. The refusals that qualify
  are `InsufficientEligibleStake` (the 875 ‰ floor), `InsufficientEligibleBonds` (after the load
  filters) and `NoOutsider`. Every other refusal still voids in the anchor block. A claim that has held
  a panel never retries. (`palw_floor_retry_is_due_v1`, `palw_sw8_thin_draws_v1`,
  `PalwStakeDrawCensusV1`.)
- **PALW-LC-14.** Below DAA 1,300, an eligibility refusal voids the claim at its anchor slot. The
  escrowed worker reward is burned, and no collateral is taken (S0).

## 7.5 Receipts, licence and deadlines

- **PALW-LC-15 (licence).** A claim MUST be licensed only by an object that meets a door of 08
  PALW-VF-13 on the backed subset of its panel (10 PALW-CO-13). The licence records door, `basis_k`
  and `escrow_released` (10 PALW-CO-11). (`palw_v2_object_licenses_claim_v1`,
  `palw_v2_licence_backed_seats_v1`.)
- **PALW-LC-16 (licence order).** The order in which a block may license claims follows
  `palw_licence_candidate_order_v2`. The known issue V04 (licences held behind claim-id order) is part
  of today's rule until it is fixed.
- **PALW-LC-17 (one deadline function).** Past `palw_rcore_plus` a claim's deadline MUST be
  `palw_rcore_deadline_v1`, the only source of a deadline for arming, rebuild and consistency checks:

  | Claim | Deadline |
  | --- | --- |
  | non-terminal, with an open **seat** DA session | none |
  | `Provisional` after a redraw | from `rebound_daa` (shifted by DA pauses) |
  | `PanelBound` | `bound_daa + window_receipt` (600 on testnet-12) |
  | `ReceiptLicensed`, open court | none |
  | `ReceiptLicensed`, `basis_k ≥ 2` | `max(licensed_daa + window_challenge_at, last_daa)` |
  | `ReceiptLicensed`, `basis_k < 2` | `max(licensed_daa + window_challenge_at, bound_daa + window_receipt + 1, last_daa)` |
  | terminal, any DA session open | none |
  | `Voided{BindTimeout}` FP on its abandon hold | `voided_daa + fp_abandon_hold_daa` |
  | terminal | `max(terminal_daa + claim_retirement_daa, last DA close + 1)` |

  `window_challenge_at` is 120 DAA past `palw_short_challenge_window`, and `window_challenge` (1,200)
  below it. `claim_retirement_daa` is 3,000.
- **PALW-LC-18 (receipt timeout).** A `PanelBound` claim that reaches its deadline without a licence:
  - on its first panel, is redrawn (uncharged);
  - on its second, voids `ReceiptTimeout` (S0′, §7.7).

## 7.6 Final, settlement and payout

- **PALW-LC-19 (Final).** A `ReceiptLicensed` claim with `basis_k ≥ 2`, no open court and no open seat
  DA session MUST become `Final` at its deadline (`palw_claim_final_floor_v1`). At Final the escrow and
  shares are written into a vesting row (10 PALW-CO-29), and the panel's liability record is
  persisted.
- **PALW-LC-20 (settlement).** PALW MUST settle on its own anchors. A payment is settled at the
  settled-anchor depth (30 on testnet-12, `palw_settled_anchor_depth`), and settlement does not depend
  on DNS finality. *Completed in the chapter 07 pass: 0127, 0129, the anchor-depth derivation.*
- **PALW-LC-21 (payout).** *Completed in the chapter 07 pass:* the pending-payout queue, payout keys,
  and the carves (10 §10.8).

## 7.7 Void reasons and what each costs

- **PALW-LC-22.** Each void reason (`PalwVoidReasonV2`) MUST be charged as follows.
  - "First panel" and "second panel" count redraws.
  - Before X10 (`palw_rcore_attributed_charging`, not declared on testnet-12), S0′ applies as listed.
  - C7 classes are always charged S0′ where the table says S0′.

  | Void | Trigger | Charge (10 PALW-CO-25) |
  | --- | --- | --- |
  | `BindTimeout` | not bound in the anchor block (and no retry applies), or the bind backstop | S0 |
  | `NoCapablePanel` | a rowed class cannot seat a panel (from DAA 750 only at the backstop) | S0 |
  | `ReceiptTimeout` | no licence by the receipt deadline, on the **second** panel (the first redraws) | S0′ |
  | `UnavailableQuorum` | `PanelUnavailableQuorum` on the **second** panel (the first redraws at once) | S0′ |
  | `NotReplayBacked` | `basis_k < 2` at its deadline on the **second** panel (the first redraws) | S0′ |
  | `ProducerWithholding` | a DA default (08 PALW-VF-24) | S1 |
  | `CourtFraud` | a court verdict, `ExecutorRefuted`, or a `PanelFalseValidV2` that acts on the claim | S2 |
  | `CourtDefault` | an unanswered court rung, or a close that never assembles | as S2; writes no `CourtConviction` record and opens no reward |
  | `CourtHeldVerdict` | a held-class court verdict | *confirm in the chapter 07 pass* |

- **PALW-LC-23.** A redraw never closes an open DA session. A post-licence S1 or S2 on a claim whose
  escrow was released also takes `E` from uncommitted stake.

**Code:** `core/palw_state_v2.rs` (`void_and_slash_at`, `palw_court_verdict_void_reason_v1`,
`palw_court_default_void_reason_v1`, `palw_void_binds_claim_v1`, `palw_escrow_destroyed_by_delta_v2`).

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis (`palw_rcore_plus`). DAA 750: `palw_panel_seed_execution`, `palw_operator_anchor`, `palw_anchor_at_ceiling`, `palw_registry_resilience`. DAA 1,300: `palw_floor_refusal_retry` |

**Sources:** ADR-0152 SR-5, SR-9, SW-8, DL-1, IA-1a, IA-1c (archive 0152/03, 06); ADR-0042 D2 (the
block state machine); ADR-0037 D2–D9 (the job state machine); ADR-0154; ADR-0155.
