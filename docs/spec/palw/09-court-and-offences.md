# PALW spec — 09. Court and offences

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. Attribution
> (J-1…J-8) is in ADR-0152, which is not on this branch.

**Purpose.** A licensed claim can still be disputed until it is `Final`. A dispute bisects the
committed execution down to one arithmetic step (or two adjacent tiles) that any node can
recompute, and adjudicates that step. Fraud proofs are unilateral: no vote, and no challenge
randomness. This chapter defines how a dispute is filed, answered and bisected, how it terminates,
the special courts (attention, held context, fused rows, checkpoints, shards), and how a verdict is
attributed to an offence. Chapter 10 has the amounts that offences slash.

**Principles served:** §3 "ran as committed". P3: only the committed execution is on trial.

## 9.1 The adjudication contract

- [ ] What a court opens, and the bound that makes it independent of model size: operands addressed in
  bytes, terminal adjudication at tile level, admission that bounds the court (a registration
  obligation, chapter 03), coverage over reachable coordinates, and decode adjudicated by challenge.
  *Sources:* 0049 A–F (as amended 2026-08-26), 0053 D1 (the court is not optional), 0069/0070 (end
  to end). *Code:* `core/palw_court_v2.rs`, `core/palw_dispute.rs`, `core/palw_terminal.rs`.
- [ ] Unilateral fraud proofs are slash-terminal, with no BFT and no challenge randomness. *Sources:*
  0027.

## 9.2 Bisection and the close

- [ ] The k-ary court, and a close that is flat in the context. *Sources:* 0082 (court decisions),
  0080 (superseded in part). *Code:* `core/palw_bisect.rs`, `core/palw_court_deadline.rs`.
- [ ] The close is assembled from what the executor served (a disputed tile, not a capture). The
  interval opening carries the fold, not the leaves. *Sources:* 0085, 0086.
- [ ] **The two-tile refutation.** When the flat pin is inadmissible, the two-tile disclosure decides
  the step. Nothing vouches for the rows beyond the root. *Sources:* 0082, 0062 §(two-tile), 0093 §9.
  *Code:* `core/palw_step_refute.rs`.
- [ ] The court ladder is minted once, and the clock is what binds. Note the two fences:
  `palw_court_ladder` is what the refutation walk reads, and `palw_context_ladder` is a separate
  fence (ADR README, "Two labels"). *Sources:* 0092, 0084 U-08, 0077 Phase B. *Code:*
  `core/palw_context_ladder.rs`.

## 9.3 Special courts

- [ ] Attention is refuted by dissection. *Sources:* 0082. *Code:* `core/palw_attn_court_v1.rs`,
  `core/palw_attn_dissect.rs`, `core/palw_attn_responder_v1.rs`, fence `palw_attn_anchored_root`.
- [ ] Held context: a held class is walked at the regime's ladder. *Sources:* 0103, 0119, and 0152
  (the held-class court, A-held). *Code:* `core/palw_held_context_v1.rs`.
- [ ] Fused rows: the court can try a fused row, and the responder is a node duty (chapter 14).
  *Sources:* 0093. *Code:* fence `palw_court_responder_coverage`.
- [ ] Checkpoint courts. *Code:* `core/palw_checkpoint_court_v1.rs`. *Sources:* TODO, name the ADR.
- [ ] The shard court: the one-move court, and the licence per shard. *Sources:* 0099 D5, 0100.
  *Code:* `core/palw_shard_court_v1.rs`. Per-shard licensing is dormant on testnet-12.

## 9.4 Offences and attribution

- [ ] The offence kinds: `ExecutorEquivocation` 0, `PanelFalseValid` 1, `CourtExecutorGuilty` 2,
  `PanelFalseValidV2` 3, `ExecutorRefuted` 4, `DaDefault` 5, `CourtConviction` 6. For each: the
  evidence that proves it, and whom it binds. *Sources:* 0152 §3.10 J-1…J-8 (F1/F2), Q-6. *Code:*
  `core/palw_offence_v1.rs` `PalwOffenceKindV1`, `core/palw_offence_attribution_v1.rs`, fence
  `palw_offence_attribution`.
- [ ] A seat that found a lie files it and files nothing else for that claim. *Sources:* 0098. *Code:*
  `core/palw_false_valid_filing_v1.rs`.
- [ ] The identity rule and contradictions 9–13, and the one resolution order. *Sources:* 0152 J-2,
  J-5.
- [ ] **Slashing evidence must be genuine on every path.** From DAA 750, the UTXO side effect of a DNS
  slash obeys the same genuineness rule as the registry path: without genuine evidence, no stake UTXO
  is removed and no reporter is paid. *Fence:* `palw_slashing_evidence_utxo_genuine` (MSK-26A).
  *Code:* `cons/pipeline/virtual_processor/utxo_validation.rs`. The DNS slash itself is in
  `spec/dns-bft`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis (courts, attribution). From DAA 750: `palw_slashing_evidence_utxo_genuine`. `palw_shard_licensing` is dormant |

**Design:** `design/palw/court.md`.
