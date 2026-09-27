# PALW spec — 06. Eligibility and block production

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions.

**Purpose.** This chapter decides which inferences are eligible, and so who produces blocks. Local
inference is unlimited, but eligibility is scarce and assigned by the protocol (P6). It is assigned
by a beacon that resolves after the work was committed (§4 of chapter 01). The ticket *is* the
execution, so the only way to draw again is to run another inference. This chapter also defines
`bits`, the difficulty window, and the two clocks every deadline in chapters 07–13 is read against:
the DAA score (the anchor's clock) and the clock cursor.

**Principles served:** P6, P3 (the ticket is the execution), and the ordering of chapter 01 §1.3.

## 6.1 The beacon

- [ ] Only attempt-class blocks carry randomness, and every lottery draws from them. A beacon costs
  one inference per re-roll. *Sources:* 0044 D4 (kept as law), 0074 D2. *Code:*
  `core/palw_fp_beacon_v3.rs`. The beacon fold is dormant on testnet-12 (`palw_beacon_fold`, refused
  by validation).
- [ ] The beacon a claim consumes does not exist when the claim's fields are fixed. *Sources:* 0144
  §4, 0072. *Code:* `core/palw_freeprompt_v3.rs`.

## 6.2 The ticket is the execution

- [ ] Both lotteries are priced in inferences, and a ticket is bound to the execution it came from.
  *Sources:* 0072 (supersedes 0071 D2), 0071 D2 bucket (only as the anchor's position field).
- [ ] A draw is one forward. *Sources:* 0117.
- [ ] The attempt envelope (V2): fields, header pins and version. *Sources:* 0042 D3, 0055
  (`PALW_ATTEMPT_V2_VERSION` 4 → 5). *Code:* `core/palw_attempt_v2.rs`,
  `core/palw_attempt_rules_v1.rs`, fence `palw_attempt_header_pins`.
- [ ] The attempt is a claim drawn by the chain. The canonical job is set by the block. *Sources:*
  0074 D1–D5, 0055.

## 6.3 The single lottery and `bits`

- [ ] One lottery. A winning attempt beats one network-wide target, and `bits` keeps the block
  interval. *Sources:* 0132 (Upgrade S, §7.5), 0137 §7, 0071 §3 (why D1 was withdrawn). *Code:*
  `core/palw_work_target_v1.rs`, `set_palw_single_lottery`.
- [ ] What remains of the per-class retarget, if anything, past `palw_work_target`. Confirm in the
  code whether `converge_idle_target_v1` (0071 D1a) still runs. *Sources:* 0071 D1a, 0137.
- [ ] **The difficulty window counts only rows priced by `bits`.** Heartbeat rows carry `bits` and
  bound the span, but are not counted. An empty count answers MAX. *Sources:* 0083 (amends 0066 D1).
  *Code:* `cons/processes/difficulty.rs`, `cons/processes/window.rs`.
- [ ] The attempt block's blue work is a constant (`1 << PALW_ATTEMPT_BLUE_WORK_LOG`). *Sources:*
  0066 D3 through 0068. *Code:* fence `palw_attempt_work`.

## 6.4 The clocks

- [ ] **The DAA score is the anchor's clock.** A block advances the DAA score if and only if `bits`
  priced it. Attempt, receipt and heartbeat blocks join the round lane outside the clock and stay
  merged and paid. *Sources:* 0138. *Code:* `palw_anchor_clock`, `core/palw_class_daa.rs`.
- [ ] **The consensus clock is a cursor.** A heartbeat consumes a slot. A block that does not advance
  the clock may not postpone it. The clock floor was added by amendment on 2026-09-24. *Sources:*
  0142 §4 and §9. *Code:* `core/palw_clock_cursor_v1.rs`, fence `palw_clock_floor`.
- [ ] The cadence: one PALW block per 120 s, frozen. *Sources:* 0038, 0042, 0137. *Code:*
  `core/palw_schedule.rs`.
- [ ] Timestamp rules: the deviation tolerance, the clock lead cap (an audit fence), and the beat-stamp
  cap (receiver clock + 132 s, operator decision of 2026-09-25). Confirm where each is enforced, and
  whether it is a validity rule or node policy. *Code:* `palw_v2_timestamp_deviation_tolerance_v1`,
  fence `palw_clock_lead_cap`.

## 6.5 Who may produce

- [ ] Production is PALW-only: no hash lane except the heartbeat clock lane of chapter 13. Two
  `algo_id` values never share a role. *Sources:* 0039 D1/D2, 0007 (the cut-off rule), 0060, 0066.
- [ ] Receipt blocks: chain position is earned, and the question is set by the block. *Sources:*
  0044 D6, 0055.
- [ ] Merged attempts count. *Sources:* 0058, 0149 §6.
- [ ] **Attempt's role shrinks** (0144 §6 item 5). State how far that has gone: today the Attempt lane
  is still the beacon source and the main reward path. *Non-normative note, with the measurements in
  Design.*

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis (single lottery, work target, anchor clock, clock cursor and floor, lead cap). `palw_beacon_fold` is dormant |

**Design:** `design/palw/lottery.md`, and `design/palw/liveness.md` for the clock.
