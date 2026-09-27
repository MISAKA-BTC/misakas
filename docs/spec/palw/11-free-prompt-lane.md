# PALW spec — 11. Free-prompt lane

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. RFC-0001 (FP
> Job V4, §A frozen) is on branch `rcore/fp-sampler`. Until it lands and is accepted, it changes
> nothing here.

**Purpose.** This is the lane ADR-0144 exists for: the user's own inference, on prompts they chose,
becomes a claim. The chapter defines the free-prompt **job** (what the user ran), its **execution
commitment**, the quantized one-shot **tickets** that a later beacon makes eligible, the **receipt**
spend, what the claim must serve so a seat can check it (the answer, never the history), the prefix
state that makes cache reuse an execution fact, the decode rules and constraints that make sampling
reproducible, and how the lane is priced. The protocol never reads what the prompt meant (chapter 01
§1.2).

**Principles served:** P1, P2, P3 (this lane is where they live), P4 (pricing is the derivation of
chapter 05), P6 (tickets).

## 11.1 The job and its commitment

- [ ] The free-prompt job: user tokens, under a registered class, from a bond. It commits prompt,
  model and parameters before execution. *Sources:* 0044 D2–D3. *Code:*
  `core/palw_freeprompt_v3.rs`, `core/palw_fp_objects_v3.rs`, `core/palw_fp_execution_v3.rs`.
- [ ] Prompt ids and their Merkle commitment. *Code:* `core/palw_prompt_ids_v1.rs`, fence
  `palw_prompt_ids_merkle`.
- [ ] The answer's shape is committed. *Sources:* 0096 (Part A).

## 11.2 Tickets and receipts

- [ ] Quantized one-shot tickets: a job's quanta, each spent at most once (`FreePrompt { quanta,
  spent }`), and each drawn by a beacon that resolves after the commitment. *Sources:* 0044 D5, 0074
  D5 (a quantum is `max(1, canonical_leaves / 8)`, now derived per chapter 05), 0144 §4. *Code:*
  `core/palw_fp_beacon_v3.rs`, `core/palw_fp_interval_v1.rs`.
- [ ] The receipt spend and the receipt block. *Sources:* 0044 D6, 0055.
- [ ] Real-demand work bears weight (①–③ in force, ④ withheld). *Sources:* 0073 as amended by 0137 and
  0144.

## 11.3 What a claim serves

- [ ] The ids ride and the capture stays home. A model-class claim serves its answer (payload
  `FPA1`: the job, the prompt ids, the answer's ids), never its history. *Sources:* 0084.
- [ ] What was made from it is committed, and the thing itself never rides. *Sources:* 0078 (D7
  withdrawn by 0144).

## 11.4 Prefix state and cache

- [ ] Cache reuse is an execution fact, never a claim. The commitment makes checkable which KV state
  was consumed, which token range was newly evaluated, and which state was produced (the
  `PalwFpPrefixStateV1` object family). *Sources:* 0145 §6, 0144 §5. On testnet-12 the V3 wire is
  still genesis. State what is armed.

## 11.5 Decode rules and constraints

- [ ] Deterministic decode: which samplers and parameters a job may use, and how a seat reproduces
  them. **On testnet-12 `palw_fp_decode_rules` (0082 D10/D11) and `palw_fp_decode_constraint` (0096
  D6–D8) are dormant, because this build cannot carry them. The validation refuses them.** Today's
  rule is the one the dormant fences leave. *Code:* `core/palw_decode_select_v2.rs`,
  `core/palw_decode_constraint_v1.rs`; `misaka-palw-constraint/`. RFC-0001 §A (`DecodeConfigV4`) is
  the proposal that would replace this.
- [ ] Ruleset caps for the lane. *Code:* fence `palw_fp_ruleset_caps`.

## 11.6 Pricing

- [ ] The free-prompt lane prices compute the same for every class, by the derivation of chapter 05.
  Paid prompt rows are kept past the claim for a window. How the entrance prices a job, and its
  budget. *Sources:* 0148 (and its addenda of 2026-09-20). *Code:* `core/palw_fp_admission_v3.rs`,
  fences `palw_fp_derived_work` and `palw_receipt_rows_unpriced`.

## 11.7 The entrance (host side)

- [ ] The entrance is the user's own machine: an OpenAI-compatible surface on loopback. A public
  commercial gateway is not a reward surface. *(node policy)* *Sources:* 0144 P1, 0077 Phase A, 0096
  Part A, 0079 (the sandbox is for the host). *Code:* `misaka-palw-gateway/`,
  `misaka-palw-fp-submit/`. Chapter 14 has the host's duties.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis, except `palw_fp_decode_rules` and `palw_fp_decode_constraint` (dormant, refused by validation) |

**Design:** `design/palw/free-prompt.md`.
