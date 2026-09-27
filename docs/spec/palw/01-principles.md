# PALW spec — 01. Principles and scope

> **Skeleton (Phase 1, 2026-09-27).** The principles below come from ADR-0144 and are stated as
> constraints. They are not validity rules of their own. Phase 2 fills §1.4 with the rules that
> enforce each principle, and with the gaps. [00-index.md](00-index.md) gives the conventions.

**Purpose.** This chapter says what PALW is for, and so what every other chapter must serve. A
proposed rule is in scope if it serves the sentence below, and out of scope if it does not (ADR-0144
Status). A rule in chapters 02–16 that contradicts a principle here is a divergence. It is recorded in
[divergences.md](divergences.md) until an ADR resolves it.

> **A person uses a local LLM on their own machine, with prompts they chose for their own reasons,
> and the same inference that answered them is the inference the chain rewards.**

## 1.1 The principles (ADR-0144 §2)

- **P1 — Local first.** Rewardable inference is inference the user ran on their own machine for their
  own use. No part of the reward path MAY assume a remote GPU marketplace. A localhost
  OpenAI-compatible endpoint is in scope. An internet-facing gateway that sells somebody else's GPU is
  not.
- **P2 — Free prompt.** The protocol MUST NOT choose the question, and MUST NOT judge whether a prompt
  was worth asking.
- **P3 — Same inference, one purpose.** The execution whose answer the user reads MUST be the execution
  the chain rewards: not a second run, and not a proof job run beside it.
- **P4 — Representation-neutral accounting.** Reward MAY change only when verified work changes. It
  MUST NOT change with `tile_len`, serialization, how a commitment is split, a registrant-declared
  multiplier, the shape of a profile, map iteration order, or any self-reported cost.
- **P5 — Efficiency is rewarded; accounting tricks are not.** The same canonical work done on faster
  hardware, a better runtime, a better quantisation or a better architecture MUST leave the miner
  more profit. Making the same work merely look larger MUST leave them nothing.
- **P6 — Local inference is unlimited; reward eligibility is scarce and protocol-assigned.**
  Eligibility is assigned by the protocol (a future beacon). The cost of spam is controlled by how
  scarce eligibility is, never by the chain forming an opinion about a prompt.
- **P7 — Model admission is permissionless; economic weight is earned through verified use.**
  - Adding a model MUST NOT require changing `main`, a hard-coded list or a model-specific fork.
  - Registrants MUST NOT choose their own reward multiplier, work value or admission strength.
  - Growth in eligibility MUST be derived from verified protocol events, never from self-reported
    popularity or compute.
  - Usage MUST NOT change the value of one unit of canonical work.
  - A new model MUST NOT change the reward rate, difficulty, coefficients or economic position of an
    unrelated model.
  - Admission rules MUST stay generic.

## 1.2 What the protocol verifies, and what it deliberately does not (ADR-0144 §3)

| The protocol verifies… | Chapter | The protocol does not verify… |
| --- | --- | --- |
| that the class was a registered one | 03 | whether the question was worth asking |
| that the inference actually ran, as committed | 04, 08, 09 | whether the answer was good |
| how much canonical work it was | 05 | who the user is |
| that the same work is not paid twice | 07, 11 | what the prompt said |
| that this inference held an eligibility ticket | 06, 11 | |

- [ ] State as a rule: no validity rule may read the meaning of a prompt, the quality of an answer
  or the identity of a user. Format, size and rate limits are allowed. *Phase 2:* list every
  validity rule that reads prompt or answer bytes, and justify each one against this rule.

## 1.3 Execute now, settle later (ADR-0144 §4)

- [ ] **Order.** The commitment over prompt, model and parameters MUST be fixed before the beacon that
  decides eligibility resolves. Eligibility MUST NOT be knowable when the work is chosen, so a ticket
  cannot be drawn first and then spent on the cheapest job (anti-grinding). *Sources:* 0144 §4,
  0044 D4, 0072, 0074 D2. *Code:* `core/palw_freeprompt_v3.rs` (the quantum ticket consumes a beacon
  that does not exist when the claim is fixed), `core/palw_fp_beacon_v3.rs`,
  `core/palw_attempt_v2.rs`.
- [ ] **The product does not wait.** Nothing in the reward path may delay the answer shown to the
  user. The claim, receipt and settlement all happen after t1 *(node policy on the host)*.
  *Sources:* 0144 §4, 0096, 0077 Phase A.
- [ ] **Ineligible work leaves no trace.** An inference that the beacon does not make eligible creates
  no obligation and no object. *Sources:* 0144 §4.

## 1.4 How each principle is enforced (Phase 2)

| Principle | Enforced in | Gaps to confirm against the code |
| --- | --- | --- |
| P1 | 11 (local entrance), 14 (host surface) | Consensus cannot tell local inference from remote. P1 is kept by the product surface and by what the reward path assumes. State this plainly |
| P2 | 11 | Confirm that no admission or validity rule reads prompt content (see §1.2) |
| P3 | 04, 06 (the ticket is the execution, 0072), 11 | Confirm that no path pays a second run or a proof job |
| P4 | 05 (derived vector, 0145/0149; no scalar coefficient, 0146) | Confirm the STEP-leaf and `tile_len` exposure of 0144 §5 is closed wherever `palw_canonical_work` is armed (testnet-12: from genesis). Property tests: `core/palw_reward_properties_v1.rs` |
| P5 | 05; 04 (backends below the semantic boundary, 0057) | — |
| P6 | 06 (beacon draw, work target W), 11 (quantized tickets) | ADR-0144 §1: in the last measurement, synthetic Attempt work carried nearly all reward. Record the current testnet-12 mix in Design, not here |
| P7 | 03 (registry, lifecycle, `admission_permille` 50/100/1,000 ‰, independence drawn) | The operator decision of 2026-09-26 (T12-030): the CLI accepts every row the chain admits, with no tool-side limit. Confirm that the Spec states the chain's admission and nothing narrower |
| §4 order | 06, 11 | — |

## 1.5 Scope rules outside ADR-0144

- [ ] **PALW settles on its own.** PALW settlement MUST NOT depend on DNS finality or on BFT validators.
  PALW terms (anchor, Final, settled) are not DNS terms (finalized, attested). *Sources:* 0127 (and
  0126 for the one coupling that stays: the DNS stake reorg gate, specified in `spec/dns-bft`).
  *Code:* `core/palw_settlement_v1.rs`, and the CI guard 0127 §5 names.
- [ ] **Out of scope.** Remote inference markets, GPU rental and third-party serving MUST NOT be
  reward surfaces (0144 P1). Withdrawn on those grounds: 0073 D2 and Phase ④, 0077's public entrance,
  0078 D7, 0079 Done-when and 0101 D4/D6/D7 (0144 §9).

**Activation.** None of its own. Principles bind every network on which PALW is on (today only
testnet-12, and devnet for drills).
**Design:** `design/palw/principles.md`: 0144 §1 (the measurement), §5 (the unit it left open, now
0145), §6 (order of work), §7 and §8 (how success is judged).
