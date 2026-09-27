# PALW spec — 11. Free-prompt lane

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/free-prompt.md](../../design/palw/free-prompt.md). The code is the truth, and
> disagreements are listed in [divergences.md](divergences.md). RFC-0001 (FP Job V4, §A frozen) is on
> branch `rcore/fp-sampler`. Until it lands and is accepted, it changes nothing here.

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (from genesis, except
`palw_fp_decode_rules` and `palw_fp_decode_constraint`, which are dormant)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P1, P2 and P3 (this is the lane ADR-0144 exists for); P4 (the price is chapter
05's derivation); P6 (tickets).

The user's own inference, on prompts they chose, becomes a claim. The protocol never reads what the
prompt meant (01 §1.2).

## 11.1 The job and its commitment

- **PALW-FP-1 (the job).** A free-prompt job MUST be the user's own tokens under a registered class,
  from a bond, with nothing appended by the protocol. It binds the prompt, the model and the
  parameters totally. It is committed by `FreePromptCommitted` (tag 12) on
  `SUBNETWORK_ID_PALW_FP_COMMITMENT` before any beacon that could make it eligible exists.
- **PALW-FP-2 (the execution commitment).** The commitment carries:
  - the execution root;
  - the full-logits trace root;
  - the output root (the rendered-output rule, unified for FP model claims, 09 PALW-CT-7);
  - `prompt_token_ids_hash`, a Merkle root over the ids, tiled for held classes
    (`palw_prompt_ids_merkle`).

  The job pin `palw_fp_job_pin_v1` is the claim's job identity.
- **PALW-FP-3 (privacy).** A job declares a privacy mode:
  - `PublicDa` (1) is the weight-bearing mode: the prompt ids are available to anyone who needs them.
  - `PanelDa` (2) sends the ids with the capture to the panel only. It is armed by height through
    `palw_panel_da`, and it requires an empty inline prompt.

  Encrypted modes do not exist.
- **PALW-FP-4 (one inference).** One inference produces the answer the user reads, the commitment and
  the capture. The answer streams to the user; the commitment does not (01 §1.3).

**Sources:** ADR-0044 D1–D3, D8, D10; ADR-0077 D2, D4, D16; ADR-0081 D3; ADR-0152 J-1, J-5. **Code:**
`core/palw_freeprompt_v3.rs` (`privacy_mode`, `PALW_FP_PRIVACY_PANEL_DA`), `core/palw_fp_objects_v3.rs`,
`core/palw_fp_execution_v3.rs`, `core/palw_prompt_ids_v1.rs`.

## 11.2 Tickets and receipts

- **PALW-FP-5 (quanta).** A committed job is credited its canonical work over new positions, split into
  one-shot quanta (05 PALW-WK-8). Each quantum is drawn by a later beacon
  (`fp_quantum_ticket_v3`, 06 PALW-EL-1) against the pooled receipt target (05 PALW-WK-14), and each is
  spent at most once (`FreePrompt { quanta, spent }`).
- **PALW-FP-6 (the receipt block).** A won quantum is spent in a receipt block (`algo_id` 7,
  `POW_ALGO_ID_PALW_RECEIPT_V3`). Its admission a full node runs with no model: the envelope, the
  ticket, the quantum not yet spent, and the bond. Chain position is earned by the draw.
- **PALW-FP-7 (the same court).** A free-prompt claim MUST be disputed in the same court as an attempt,
  and real-demand work bears weight: ADR-0073 ①–③ are in force, and Phase ④ is withheld. A claim
  becomes `Final` and is paid as chapter 07 says.

**Sources:** ADR-0044 D4–D6, ADR-0074 D1–D2, ADR-0073 D1–D3 (④ withheld by ADR-0144), ADR-0148.
**Code:** `core/palw_fp_beacon_v3.rs`, `core/palw_fp_admission_v3.rs`.

## 11.3 What a claim serves

- **PALW-FP-8 (the answer, never the history).** A model-class claim serves its answer: the `FPA1`
  envelope, carrying the job, the prompt ids and the answer's ids. It never serves its history. A node
  serves the envelope whenever the capture does not fit the carrier, and nothing over the cap is pushed.
- **PALW-FP-9 (the interval opening).** A free-prompt seat verifies one interval of positions, and the
  executor opens it. The opening is V4: it carries the fold's digests and frontier, never the leaf
  hashes (09 PALW-CT-18). The interval arm serves both lanes. History is replayed from a checkpoint.
- **PALW-FP-10 (derived artifacts).** What was made from an answer (a derived artifact: a DSL output, a
  model file, a converted asset) is committed as a derivation: the output canonicalized by a registered
  grammar, and content-named pure transformers. The artifact itself never rides. Verifying a derived
  artifact belongs to its consumer. **A derivation transformer earns no PALW weight unless it is the
  user's local inference itself** (ADR-0144, withdrawing ADR-0078 D7).

**Sources:** ADR-0084 D1–D7, ADR-0077 D8, D10–D11, ADR-0086, ADR-0078 D1–D6 and D8–D11. **Code:**
`core/palw_fp_interval_v1.rs`, `core/palw_fp_objects_v3.rs`; node side `kaspad/src/palw_fp_seat.rs`,
`kaspad/src/palw_retention.rs`.

## 11.4 Prefix state and cache

- **PALW-FP-11.** A cache hit MUST be an execution fact, never a claim. A job that reuses a prefix
  commits which KV state it consumed, which token range it newly evaluated, and which state it produced
  (`PalwFpPrefixStateV1`), so a seat that re-runs it can check all three. Only new positions are
  credited (05 PALW-WK-2).

**Sources:** ADR-0145 §6, ADR-0144 §5. **Code:** `core/palw_freeprompt_v3.rs`, `core/palw_fp_admission_v3.rs`,
`core/palw_lifecycle_objects_v2.rs` (`PalwFpPrefixStateV1`).

## 11.5 Decode rules and constraints

- **PALW-FP-12 (today's decode).** Decoding is a seeded argmax over the committed logits. A decode pin
  challenges the argmax, and on a tiled class the two-tile refutation applies (09 PALW-CT-20). The
  answer is cut at the first committed end-of-generation id, by a versioned rule.
- **PALW-FP-13 (dormant on testnet-12).** Two rules exist in the code but are refused by validation,
  because this build cannot carry them:
  - the richer decode rules of ADR-0082 D10/D11 (`palw_fp_decode_rules`), under which a job's decode
    leaves earn its quanta;
  - the decode constraint of ADR-0096 D7, where the committed token is the admitted one
    (`palw_fp_decode_constraint`; `misaka-palw-constraint/`).

  RFC-0001 §A (`DecodeConfigV4`) is the proposal to replace them.
- **PALW-FP-14 (caps).** The lane's ruleset caps apply (`palw_fp_ruleset_caps`).

**Sources:** ADR-0082 D10–D12, ADR-0096 D6–D8, ADR-0152 J-8. **Code:** `core/palw_decode_select_v2.rs`,
`core/palw_decode_constraint_v1.rs`, `core/palw_step_refute.rs`.

## 11.6 Pricing

- **PALW-FP-15.** The free-prompt lane MUST price compute exactly as chapter 05 does for every class:
  - credit over new positions;
  - one network quantum;
  - quanta that scale odds, not credit;
  - a spend that weighs one network quantum;
  - a reservation equal to the claimed compute.

  Paid prompt rows are kept past the claim for a bounded time. The entrance prices a job with the
  ledger's own expression, and sizes its budget in the same unit.

**Sources:** ADR-0148 §2–§3 and its 2026-09-20 addenda. **Code:** `core/palw_fp_admission_v3.rs`,
fences `palw_fp_derived_work`, `palw_receipt_rows_unpriced`.

## 11.7 The entrance (host side)

- **PALW-FP-16 (local only).** The entrance MUST be the user's own machine: one OpenAI-compatible
  surface on loopback (`/v1`), served by the Studio and the gateway alike. A public commercial gateway
  is not a reward surface (P1). *(node policy)* The entrance:
  - reads the chain it commits to (genesis ∪ chain classes);
  - maps sampling requests with notice, never silently;
  - serves `response_format` in two stated enforcement modes;
  - treats a tool call as a turn of text;
  - reports a long thread as a chain of jobs.

  It says the model's limits before the first token (14 §14.4).

**Sources:** ADR-0144 P1, ADR-0077 D1–D5 (public entrance withdrawn by 0144), ADR-0096 Part A (D1–D5,
D9) and Part C (D10–D13), ADR-0079. **Code:** `misaka-palw-gateway/`, `misaka-palw-fp-submit/`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis, except `palw_fp_decode_rules` and `palw_fp_decode_constraint` (dormant, refused by validation). `PanelDa` needs `palw_panel_da` |
