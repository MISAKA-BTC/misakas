# PALW spec — 08. Verification

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/verification.md](../../design/palw/verification.md). The code is the truth, and
> disagreements are listed in [divergences.md](divergences.md).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (from genesis, with the DAA-750
seed and maturity fences)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** §3 "ran as committed", P7 (independence is drawn, not declared).

For each claim, a **panel** of bonded seats is drawn from operators other than the executor. Seats
re-execute and file receipts, which are evidence, not votes. Receipts are counted into a licence by
independent attesters per segment. A missing unit of data is contested in a DA session. A seat that
finds a lie takes it to the court or files an offence ([09](09-court-and-offences.md)).

## 8.1 The panel seed

- **PALW-VF-1.** For a claim whose **anchor block's DAA** is at or past `palw_panel_seed_execution`
  (DAA 750 on testnet-12), the draw seed MUST be
  `H("misaka-palw/panel-v2/draw-seed/v1" ‖ anchor attempt's execution_commitment_v3 ‖ claim_id)`
  (`palw_panel_draw_seed_v1`). Below that height the seed is the anchor block's identity.
- **PALW-VF-2.** The seed is stored as the panel's `anchor` (`PalwPanelStateV2::anchor`). The segment
  assignment, the S3 sample, the S2 optimistic door and the leaf-evidence interval draw MUST all read
  that stored seed. Nothing may treat `panel.anchor` as a block hash.
- **PALW-VF-3.** Which block anchors a claim, and the operator-anchor stopgap from DAA 750, are in
  [07](07-claim-lifecycle.md) §7.3.

**Sources:** the 2026-09-25 correction to ADR-0152 SW-8
([t12-panel-seed-2026-09-25.md](../../t12-panel-seed-2026-09-25.md)); ADR-0154. **Code:**
`core/palw_panel_v2.rs` (`palw_panel_draw_seed_v1`); the processor's anchor walk
(`palw_v2_anchor_fact_with_seed_v1`, `palw_panel_anchor_execution_v1`).

## 8.2 The stake-weighted draw (SW)

Past `palw_rcore_plus` (testnet-12, from genesis), the flat class draw and ADR-0147's outsider seat
are stake-weighted. The admission jury is not (03 §3.3).

- **PALW-VF-4 (SW-2, weight).** An operator's weight MUST be
  `W = min(1,000,000, ⌊collateral / SOMPI_PER_MSK⌋)`, where the collateral is its one eligible bond's
  posted collateral (`PALW_DRAW_WEIGHT_CAP_MSK_V1`). Eligibility is decided separately by the one
  ledger and the Valid-lock filter. Only a slash lowers a weight after the anchor.
- **PALW-VF-5 (SW-3, key).** For each eligible operator, `u` is the first 8 bytes (little-endian) of
  `H(domain ‖ anchor seed ‖ claim_id ‖ operator_id)`:
  - class seats use domain `misaka-palw/panel-v2/stake-ticket/v1`;
  - the outsider uses `misaka-palw/panel-v2/stake-outsider-ticket/v1`.

  The key is `L / W`, with `L = palw_draw_neg_log2_q64_v1(u)`, the normative integer −log2 in Q64.64.
  Keys compare by `L_i · W_j < L_j · W_i` in u128, and ties go by `operator_id`. The panel is the
  `needed` smallest keys in key order. The outsider (the smallest outsider key, drawn from the base-class
  population minus the registrant) comes first when the class has one.
- **PALW-VF-6 (SW-1, unchanged inputs).** The rest of the draw is unchanged:
  - eligibility: seat floor, headroom, maturity, registered before the anchor, not the executor,
    capable, ready, and the Valid-lock filter;
  - one seat per operator;
  - the S1 segment assignment (`palw_segment_assignment_v2`) and the S3 sampler sites.
- **PALW-VF-7 (SW-10, eligible-stake floor).** The draw MUST refuse with `InsufficientEligibleStake`
  unless `(eligible + X) × 1000 ≥ (base + X) × 875`:
  - `base` is the capped weight of every operator that could sit (Active, at the seat floor,
    registered before the anchor, capable), whether or not the ledger admits it;
  - `eligible` is the same sum over those that also pass the headroom and Valid-lock filters;
  - `X` is the executor operator's capped weight when it could sit but for being the executor, else 0.

  Fewer eligible operators than seats refuses with `InsufficientEligibleBonds`. What a refused draw
  does to the claim, a void before DAA 1,300 and a retry from 1,300, is in 07 §7.4.
- **PALW-VF-8 (SW-8, one state).** Derivation, acceptance and the fold MUST read the same state: the
  pre-object base of the anchor block, advanced by that block's earlier bindings in claim-id order, with
  the draw policy resolved at the anchor DAA (`palw_v2_derived_panel_bindings_on_one_state_v1`). A
  `PanelBound` carried in any other block is refused.
- **PALW-VF-9 (SW-9, rate room).** A class's panel capacity MUST count effective ready operators
  `ready_eff = min(ready operators, max(seat_count, ⌊ΣW_ready / W_max,ready⌋))` over capped weights
  (`palw_panel_ready_eff_v1`), in `panel_rate_v1`, `panel_room_v1` and op 186 alike.
- **PALW-VF-10.** One operator id backs at most one bond (`palw_operator_id_unique`), which the weight
  rule relies on.
- **PALW-VF-11.** With `stake: None` (every network but testnet-12), the draw is ADR-0130's operator
  lottery, byte for byte.

**Sources:** ADR-0152 §3.14 SW-1…SW-10, IA-1/IA-1b (archive 0152/06); ADR-0147 (the outsider);
ADR-0130 (the lottery it replaces on testnet-12). **Code:** `core/palw_panel_v2.rs`
(`PalwPanelDrawPolicyV1`, `palw_panel_stake_ticket_v1`, `palw_panel_stake_outsider_ticket_v1`,
`palw_panel_stake_entries_v1`, `palw_panel_stake_floor_v1`, `palw_panel_stake_executor_bonds_judging_v1`,
`palw_draw_neg_log2_q64_v1`, `palw_draw_operator_weight_msk_v1`, `palw_panel_ready_eff_v1`); the
processor's resolver `palw_panel_draw_policy_at`.

## 8.3 Seats and readiness

- **PALW-VF-26 (who may sit).** A bond MAY be drawn for a claim only if all of the following hold:
  - it is `Active`, at or above the seat floor (130,000 MSK on testnet-12: ten producer floors,
    `palw_panel_collateral_floor_v1`), and mature (10 PALW-CO-7);
  - it was registered before the claim's anchor;
  - it is not the executor's bond or operator;
  - it is capable of the class (`palw_capability_bound`, ADR-0071 D3);
  - it is ready for the class (below);
  - its room covers `max(duty_bind, lock_2)` (10 PALW-CO-20).
- **PALW-VF-27 (readiness V2).** A seat is ready for a class only if it holds a current
  `SeatReadinessProvedV2` (tag 50). The proof opens `PALW_READINESS_V2_CHUNKS_V1` = 16 distinct leaves,
  drawn from the whole artifact by `palw_readiness_v2_leaves_v1` under a seed that is a function of
  (class, bond, span). The leaves are opened in one multiproof (`PalwArtifactMultiproofV1`), and the
  proof is signed over the opened leaves' hashes, not only their indices. A proof counts for
  `palw_readiness_v2_max_age_spans` spans: 24 on testnet-12
  (`PALW_READINESS_V2_MAX_AGE_SPANS_T12_V1`), 8 elsewhere. The V1 one-leaf proof is refused past
  `palw_readiness_v2`.
- **PALW-VF-28 (readiness terms).** Ready also requires:
  - a matching artifact root;
  - the chunks held;
  - participation;
  - collateral for `readiness_collateral_multiple` seat exposures;
  - a probe verified within `readiness_probe_max_age_spans`.

  (`palw_model_registry_v1.rs`.)
- **PALW-VF-29 (class-local fail-closed).** A class whose bound claims cannot be served by its ready
  seats stops only itself (`Held`, 03 §3.3). A class whose derived verification window does not fit
  the receipt deadline is `Held`, never `Active`. Past `palw_class_receipt_window` its receipt
  deadline is `bound_daa + max(window_receipt, verification_window_spans × span_daa)`
  (`receipt_window_for_claim_v1`). On testnet-12 the class-derived deadline `D(c)` of
  `palw_class_verify_deadline` (03 §3.6) governs.
- **PALW-VF-30 (coverage and shards).** A class's coverage — the probability that the panel's checks
  catch a lie of a given size — is a generated number (`palw_seat_coverage_v1`), reported, not
  assumed. Shard seats and per-shard licensing (ADR-0099 D5, ADR-0100 D4) are dormant on testnet-12.
- **PALW-VF-31 (routing).** Receipts carry the routing keys (execution-class family and model band),
  and the registry, not the miner, gives them meaning (ADR-0034). Only the CPU integer family
  adjudicates (ADR-0053).

**Sources:** ADR-0133 §3 D3, §11.2, §11.3; ADR-0152 (readiness horizon 24); ADR-0124 D4; ADR-0071 D3;
ADR-0098 D1; ADR-0034. **Code:** `core/palw_model_registry_v1.rs`, `core/palw_artifact.rs`
(`PalwArtifactMultiproofV1`, `verify_artifact_multiproof_v1`), `core/palw_seat_coverage_v1.rs`,
`core/palw_readiness_escalation_v1.rs`, `core/palw_routing.rs`.

## 8.4 Receipts and quorum (Q)

- **PALW-VF-12 (Q-1, verdicts).**
  - **`Valid` in a V2 receipt** attests a full replay.
  - **`Valid` in a V3 receipt** attests an S1 replay of exactly the segments in its mask.
  - **`Sampled`** (past `palw_rcore_plus`) attests that the seat held and recomputed its S3 sites. It
    MUST NOT count toward any quorum or coverage. It takes no lock and carries no liability. It is not
    "served" (10 PALW-CO-9) and latches `unserved_seen`. It does not satisfy the outsider veto. It is
    credited for seat pay. Below the fence `Sampled` is refused.
- **PALW-VF-13 (Q-2, doors).**
  - **V1 `ReceiptLicensed`:** at least 3 `Valid` V2 receipts plus the outsider.
  - **Coverage `ReceiptLicensedV2`:** a quorum of 3 `Valid`, and every segment attested by at least 2
    `Valid` masks. In the shipped geometry that means all five seats.
  - **`OptimisticLicensed` (S2):** the full seat's `Valid` alone, as a fast path.
  - **ShardPart:** dormant on testnet-12.
- **PALW-VF-14 (Q-3, recount).** The licence records
  `basis_k = min(3, min over segments of #distinct counted Valid signers covering it)`
  (`palw_receipt_set_basis_k_v1`). A supplementary set may raise it, never lower it. **Final requires
  `basis_k ≥ 2`** (`PALW_RCORE_FINAL_BASIS_K_V1`).
- **PALW-VF-15 (Q-5, S2 is never the basis of Final).** An S2 claim is `ReceiptLicensed` with
  `basis_k = 1`, escrow held and replay still charged. It upgrades when supplementary receipts raise
  the recount to 2 or more. The V3 door (SR-10) takes partial seats' V3 receipts, and the V2 door takes
  any seat's full-replay V2 `Valid`. The upgraded door is recorded as Coverage if a counted mask is
  partial, else as Quorum. If `basis_k < 2` still holds at its deadline:
  - on the first panel the claim redraws;
  - on the second it voids `NotReplayBacked` (07 §7.7).
- **PALW-VF-16 (Q-4, Q-6).**
  - Each counted `Valid` signer locks `lock_{max(basis_k,2)}` with its mask (10 PALW-CO-16).
  - A `Full` receipt is always liable.
  - A `Segmented` receipt is liable by its mask and the fault's site. The sites are `Leaf` for step
    arithmetic and structure, and `Whole` for shape, checkpoint, forged output, output mismatch,
    withholding and court fraud.
  - `Sampled` is never liable.
- **PALW-VF-17.** An `Unavailable` seat abstains from fork choice, and it is not a verdict
  (`palw_unavailable_abstains`, 0065 D4).
- **PALW-VF-18 (SR-9).** A `PanelUnavailableQuorum` object (tag 56) carrying `seat_count − quorum + 1`
  signed `Unavailable` receipts of the current panel redraws a first panel at once. On a second panel it
  voids the claim `UnavailableQuorum`.

**Sources:** ADR-0152 §3.12 Q-1…Q-7, SR-9, SR-10 (archive 0152/03, 06); ADR-0108 (a receipt is
evidence). **Code:** `core/palw_receipt.rs`; `core/palw_state_v2.rs` (`palw_receipt_set_basis_k_v1`,
`palw_seat_uncounted_on_licence_v1`, `palw_rcore_counts_licensed_v1`); `core/palw_panel_v2.rs`
(`palw_select_optimistic_licence_v2`, `palw_select_coverage_licence_v2`).

## 8.5 Replay and verification clocks

- **PALW-VF-32 (three clocks).** Verification MUST run on its own clock, apart from the execution round
  (1 s) and the PALW anchor (120 s). A panel holds its claim across the receipt window (600 DAA on
  testnet-12), not across one anchor.
- **PALW-VF-33 (Verification V2, S1 segments).** Past `palw_verification_v2`:
  - A job's leaves are cut into `K = seats − 1` equal segments (`palw_segment_leaf_range_v2`).
  - The bind names one full-replay seat and gives every other seat its segments by a drawn rotation
    (`palw_segment_assignment_v2`, a function of the bind and the stored seed).
  - A V3 receipt names the segments it attests, and its signature covers the mask
    (`palw_receipt_message_v3`).
  - The coverage door (08 PALW-VF-13) needs every segment attested `Valid` at least twice
    (`PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT` = 2).
  - The assignment is a seat's duty, not a cap on what it may attest.
- **PALW-VF-34 (a seat that found a fault).** A seat that proves a fault in a claim records it in its
  fault ledger, and MUST file nothing else about that claim, in that round or any later one. Every
  proven fault is recorded at the most specific address known: a checkpoint-root mismatch is `(0, 0)`,
  and bisection finds the leaf. *(node policy)*
- **PALW-VF-35 (leaf demand).** A seat that needs a committed leaf to judge first asks the executor off
  chain. If no answer arrives within `PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1`, it demands the leaf on chain
  (`DefaultAccusedHeld` with `StepLeaf`, or a DA session past `palw_rcore_plus`, 08 §8.6). The answer is
  adjudicated by the one-move verdict. Below `palw_rcore_plus`, at most one demand per seat per claim,
  for a leaf in that seat's sample (ADR-0111 D7).
- **PALW-VF-36 (sampling schedules, never convicts).** Sampling decides who re-executes what and when.
  A conviction needs a refutation or a DA default, never a sampling result. *Sources:* ADR-0028 §1,
  §5, with windows restated by ADR-0133.

**Sources:** ADR-0133 D1, §11.1; ADR-0098 D2–D3; ADR-0111; ADR-0028. **Code:**
`core/palw_verification_v2.rs`, `core/palw_replay_refute_v1.rs`, `core/palw_leaf_evidence_v1.rs`,
`core/palw_fp_interval_v1.rs`; node side `kaspad/src/palw_panel.rs`.

## 8.6 Data availability (DA)

Past `palw_rcore_plus`:

- **PALW-VF-19 (DA-1, DA-2).** `DefaultAccused` (event units) and `DefaultAccusedHeld` (held units)
  MUST open sessions keyed by `(claim, accuser)`, on every class, floor included.
  - A session never changes the claim's phase: `DefaultDisputed` is never written.
  - The v1 answers `MaterialDisclosed` and `MaterialDisclosedHeld` are refused.
  - Sessions live in `da_sessions` and `da_claims`.
- **PALW-VF-20 (DA-3, units).** A session holds the named unit plus up to 3 drawn units
  (`PALW_DA_DRAWN_UNITS_V1`):
  - The draw is seeded by `H(PALW_DA_DRAW_DOMAIN_V1 ‖ accepting block ‖ claim ‖ accuser)`.
  - Drawn units lie **inside the committed run**: event rows below `decode_rows`, and held units for
    the attempts of held-context classes.
  - A named `StepLeaf` may be any leaf inside the binding's bound.
  - A unit whose checker needs dissection is refused (`DaUnitNeedsDissection`); it goes to the court.
  - A held accusation whose binding fails the identity rule is refused (`DaBindingIsIdentityFault`).
- **PALW-VF-21 (DA-4, answers).** `MaterialDisclosedV2` (tag 55), signed by the discloser, answers a
  unit. The discloser may be the producer or any bond with a live lock on the claim. An answer is
  checked by the hash arithmetic against the claim's roots and by the identity rule. A `Flat` answer
  covers every in-run event row. A session whose units are all answered is **refuted**.
- **PALW-VF-22 (DA-5, pause and re-key).**
  - **Seat sessions and the deadline:** only a session opened by a seat of the current panel pauses a
    pre-Final claim. While it is open the claim has no deadline, and on close its anchor
    (`rebound_daa`, `bound_daa` or `licensed_daa`) shifts by the pause.
  - **Rows and locks:** opening any session on a claim with a vesting row re-keys the row to at least
    `deadline + window_challenge_at`, and re-dates the signers' live locks with it.
  - **Retirement and redraws:** retirement waits for open sessions, and a redraw never closes one.
- **PALW-VF-23 (DA-6, cost).** An accuser stakes
  `exposure = min(⌈r × S_P(stage)⌉, floor)` on its free half, with `r` = 1,000 bps, where `S_P(stage)`
  is the debit a default at that stage would trigger.
  - A refuted session's exposure is held. It is returned if the claim is convicted before retiring,
    and burned otherwise.
  - A confirmed or released session returns its exposure.
- **PALW-VF-24 (DA-7, default).** At step 2, the first session past its deadline with an unanswered
  unit confirms withholding, and the claim's other sessions close.
  - **The effect by stage:**
    - *unlicensed:* void `ProducerWithholding`, charged S1;
    - *licensed:* S1 at the post-licence price, S4 on covering signers, and the claim voids;
    - *Final with an unmatured row:* the row is burned (S3), plus the producer's action, and S4 on
      covering signers; the Final is reversed.
  - **The record:** a `DaDefault` (kind 5), with root 0 and forfeiture by claim.
  - **Covering signers** are full-mask `Valid` signers with a live lock
    (`palw_da_unit_covered_by_v1`). Their S4 is live, because `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1`
    is `true`.
  - **The reward** goes to the accuser of the earliest defaulted session.
- **PALW-VF-25 (DA-8, windows and caps).** A claim is accusable from `PanelBound` through Final while
  its record exists and its row is unmatured, and only if `now + W_disclose ≤ trace_retention_daa`.
  The caps:
  - a current seat has at most one open session and 4 over the claim's life;
  - non-seats have at most 3 open and 16 in total;
  - an accuser is Active, at or above the floor, not the producer, and has A-6 room.

**Sources:** ADR-0152 §3.11 DA-1…DA-9 (archive 0152/05), amending ADR-0062 on testnet-12. Below the
fence, ADR-0062 SA-1…SA-7 and ADR-0111 D3 stand. **Code:** `core/palw_da_rcore_v1.rs`,
`core/palw_state_v2.rs` (`sweep_da_sessions`, `da_default_charge_v1`), `core/palw_panel_da_v1.rs`,
`core/palw_operator_da_v1.rs`, `core/palw_held_da_v1.rs`.

## 8.7 What the panel is paid, and what a seat holds

- **PALW-VF-37 (the pool).** Past `palw_panel_economy`, a `Final` claim's reward `R` is split: the panel
  pool is `⌊R × 200 / 1000⌋` (`PALW_PANEL_POOL_PERMILLE_V1`), and the producer takes the exact rest.
  `R` is the escrow after work pricing and after the buyback slice. Nothing is minted beyond the carve
  the accepting block withheld.
- **PALW-VF-38 (a seat's share).** Each drawn seat whose `Valid` (or `Sampled`) receipt the chain
  credited inside the receipt window is paid one fixed share, `⌊pool / K⌋`, where K is the number of
  seats drawn. Unpaid shares go to the panel reserve, never to the producer. On testnet-12 the shares
  vest in the claim's row (10 §10.6).
- **PALW-VF-39 (what a seat holds).**
  - **testnet-12:** a seat's duty is `duty_bind` (10 PALW-CO-19), and its lock is L-1's. The earlier
    `3 × claim.reserved` exposure is retired.
  - **Other networks:** past `palw_panel_exposure_floor` a seat reserves
    `max(3 × claim.reserved, λ × max_seat_reward)`. On testnet-12, λ = 5 (`reward_multiple_permille`
    5,000), which enters `duty_bind`'s λ-term.
- **PALW-VF-40 (work-priced reward).** Past `palw_work_priced_reward`, a model-class claim is paid
  `⌊escrow × min(pwu, unit) / unit⌋`, where `unit` is the heaviest weight-bearing class's canonical
  inference. The unnamed remainder is never minted (`palw_work_priced_reward_v1`).

**Sources:** ADR-0124 D1–D4, D6 (D3 is superseded on testnet-12 by ADR-0152 A-4; D5 by SW); ADR-0130
D1–D2. **Code:** `core/palw_panel_economy_v1.rs`, `core/palw_state_v2.rs` (`palw_panel_payout_key_v1`).

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | stake draw, Q and DA from genesis (`palw_rcore_plus`). DAA 750: `palw_panel_seed_execution`, `palw_bond_maturity_early`. `palw_shard_licensing` dormant (refused by validation) |
