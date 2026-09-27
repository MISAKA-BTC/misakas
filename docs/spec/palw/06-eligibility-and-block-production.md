# PALW spec — 06. Eligibility and block production

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/lottery.md](../../design/palw/lottery.md) and, for the clock,
> [design/palw/liveness.md](../../design/palw/liveness.md). The code is the truth, and disagreements
> are listed in [divergences.md](divergences.md).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (single lottery, work target, anchor
clock, clock cursor and floor, all from genesis)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P6 (eligibility is scarce and protocol-assigned); P3 (the ticket is the
execution); chapter 01 §1.3 (the beacon resolves after the work is committed).

Which inferences are eligible, and so who produces blocks. The ticket *is* the execution: the only way
to draw again is to run another inference. This chapter also defines the two clocks every deadline is
read against, the DAA score and the clock cursor.

## 6.1 The beacon

- **PALW-EL-1.** Only attempt-class blocks carry randomness, and every lottery MUST draw from them. A
  free-prompt quantum's ticket (`fp_quantum_ticket_v3`) consumes a beacon, an attempt-class chain block,
  that does not exist when every field of the claim is fixed. A beacon costs one inference per re-roll.
- **PALW-EL-2.** The beacon fold (`palw_beacon_fold`) is dormant on testnet-12: validation refuses it.

**Sources:** ADR-0044 D4 (kept as law), ADR-0074 D1–D2, ADR-0144 §4. **Code:** `core/palw_fp_beacon_v3.rs`,
`core/palw_freeprompt_v3.rs`.

## 6.2 The ticket is the execution

- **PALW-EL-3 (the envelope).** An attempt MUST be a `PalwAttemptEnvelopeV2` of version
  `PALW_ATTEMPT_V2_VERSION` = 6 in an attempt block (`algo_id` 6, or 9 for the execution family). Its
  identity is `attempt_id`, and the commitment binds the PoW.
- **PALW-EL-4 (the priced bytes).**
  - The execution commitment is
    `execution_commitment_v3(attempt, anchor) = H(domain ‖ anchor ‖ borsh(attempt with challenge := 0))`.
  - Every other field is priced, and every priced field is pinned by a rule or it is the challenge
    (`palw_attempt_header_pins`).
  - The anchor is derived from the header, never carried:
    `execution_anchor_v3(network, pre_pow_hash, class_id, bond, nonce)` = the job anchor at the nonce's
    bucket (`PALW_TICKET_NONCE_BUCKET_LOG2` = 22).
- **PALW-EL-5 (both draws come from one execution).** The class ticket is `class_ticket_v3(attempt, anchor)`,
  the low 128 bits of `H(domain ‖ execution_commitment_v3)`. The Layer-0 digest is taken over the same
  commitment. One draw is one execution: `pwu = max(1, expected_draws(target)) × per_inference`, which
  past `palw_canonical_work` is the derivation (05 PALW-WK-7).
- **PALW-EL-6 (a draw is one forward).** The prefill is one pass over the weights. Past
  `palw_prefill_draw`, a draw's job is the prefill plus one decode step (`exact_decode_tokens` = 1 on
  testnet-12).
- **PALW-EL-7 (the job is set by the block).** The attempt's canonical job, and a receipt block's
  question, are set by the chain from the anchor. A producer cannot choose them. Model classes have a
  fixed canonical job (09 PALW-CT-8).

**Sources:** ADR-0072 D1–D8, ADR-0042 D3, ADR-0055, ADR-0074 D1–D4, ADR-0117, ADR-0071 D2 (as the
position field only). **Code:** `core/palw_attempt_v2.rs`, `core/palw_attempt_rules_v1.rs`
(`palw_attempt_context_v1`, `palw_attempt_canonical_v1`).

## 6.3 The single lottery

- **PALW-EL-8.** Past `palw_single_lottery` (testnet-12 from genesis, set together with
  `palw_anchor_clock` by `set_palw_single_lottery`), the class ticket is the whole lottery:
  - a PALW attempt header passes Layer-0 unconditionally and derives block level 0;
  - an attempt row does not price the difficulty window;
  - the class ticket beats `MAX · min(1, CCU / max(W₀, W))`, where `W₀` is the block's escrow over the
    rate and `W` is the rooted work target (05 PALW-WK-9, PALW-WK-10; `palw_work_lottery_floor_v1`).
- **PALW-EL-9 (priced lanes).** Which lanes `bits` prices is `algo_id_is_priced_by_bits_v3`. On a
  `ConsensusV2` network past the single lottery, no producible lane is priced by `bits`: attempt,
  execution, receipt and round lanes are out, and every V1 PoW activation is `never()`. Heartbeat rows
  carry `bits` and bound the span, but the difficulty window does not count them. An empty count
  answers MAX.
- **PALW-EL-10 (what remains of per-class targets).** Past `palw_work_target` a class has no target of
  its own. The floor class keeps its own DAA and target as the residual (05 PALW-WK-11), with an idle
  class converging toward the producing classes' price, never past it (`converge_idle_target_v1`).

**Sources:** ADR-0132 §7.5 (Upgrade S), ADR-0137 D1–D3, ADR-0083 D1, ADR-0138, ADR-0071 D1a (D1
withdrawn). **Code:** `core/palw_work_target_v1.rs`, `core/palw_class_daa.rs`,
`cons/processes/difficulty.rs`, `cons/processes/window.rs`.

## 6.4 The clocks

- **PALW-EL-11 (the DAA score is the anchor's clock).** Past `palw_anchor_clock`, a block MUST advance
  the DAA score exactly when `bits` priced it. Attempt, receipt, heartbeat and round blocks join the
  round lane outside the clock. They stay merged, blue where their lane is blue, paid and folded.
  Because no producible lane is priced on testnet-12 (PALW-EL-9), the stand-in rule counts **exactly one
  heartbeat** per mergeset that carries one, at most once per slot. testnet-12's DAA score therefore
  ticks with the heartbeat clock (10 PALW-CO-44).
- **PALW-EL-12 (the cursor).** The clock is a cursor (`PalwClockCursorV1 { next_slot_ms, slots_consumed }`):
  - a heartbeat is admissible iff `header.timestamp ≥ next_slot_ms`;
  - an admitted heartbeat moves the cursor to the first slot boundary strictly after its timestamp;
    missed slots are lost, not banked;
  - no other lane writes the cursor, so a block that does not advance the clock cannot postpone it.

  The clock floor (amendment of 2026-09-24, `palw_clock_floor`) applies.
- **PALW-EL-13 (cadence).** The PALW cadence is one block per 120 s, frozen for testnet and mainnet.
- **PALW-EL-14 (timestamps).** A V2 header's timestamp MUST be within
  `palw_v2_timestamp_deviation_tolerance_v1` and within the clock lead cap (`palw_clock_lead_cap`).

**Sources:** ADR-0138 §2, ADR-0142 §4 and §9, ADR-0038 H, ADR-0151 D3. **Code:** `core/palw_clock_cursor_v1.rs`,
`cons/processes/difficulty.rs` (`lane_advances_daa_at`, `daa_exempt_count`), `config/params.rs`
(`palw_anchor_clock`, `palw_clock_floor`, `palw_clock_lead_cap`).

## 6.5 Who may produce

- **PALW-EL-15 (PALW-only).** Block production MUST be PALW work: attempt, receipt, execution and round
  lanes. The heartbeat is the clock (13 §13.3). There is no hash production lane, and two `algo_id`
  values never share a role. Total PALW unavailability halts production loudly; the clock keeps
  moving, and nothing degrades to hash ordering.
- **PALW-EL-16 (bonded, not permissioned).** Any bond at the producer floor MAY produce (10
  PALW-CO-4). The floor class is the liveness floor, portable and integer-only (`PALW-BASE-0`).
- **PALW-EL-17 (merged attempts count).** Attempts in merged blues are claims (02 PALW-ST-15).
- **PALW-EL-18 (Attempt's role).** *Non-normative:* today the Attempt lane is both the beacon source and
  most of the reward path. ADR-0144 §6 item 5 is the rule for shrinking it. Measure the mix in Design.

**Sources:** ADR-0039 D1, D2, D4, D6; ADR-0038 A, E; ADR-0007 (the cut-off rule); ADR-0060; ADR-0058.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis: attempt header pins, prefill draw, single lottery, anchor clock, work target, clock cursor and floor, lead cap. `palw_beacon_fold` is dormant |
