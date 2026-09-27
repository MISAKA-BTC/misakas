# PALW spec — 01. Principles and scope

> **Normative, as constraints.** The principles below come from ADR-0144, the constitution. They are
> not validity rules of their own: they constrain every rule in chapters 02–16, and §1.4 lists, for
> each principle, the rules that enforce it and the gaps that remain. Reasoning:
> [design/palw/principles.md](../../design/palw/principles.md). A rule that contradicts a principle is
> a divergence, and it goes into [divergences.md](divergences.md).

**Applies to:** every network on which PALW is on (testnet-12, and devnet for drills)

> **A person uses a local LLM on their own machine, with prompts they chose for their own reasons,
> and the same inference that answered them is the inference the chain rewards.**

## 1.1 The principles

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
- **P6 — Local inference is unlimited; reward eligibility is scarce and protocol-assigned.** Eligibility
  is assigned by the protocol, through a future beacon. The cost of spam is controlled by how scarce
  eligibility is, never by the chain forming an opinion about a prompt.
- **P7 — Model admission is permissionless; economic weight is earned through verified use.**
  - Adding a model MUST NOT require changing `main`, a hard-coded list or a model-specific fork.
  - Registrants MUST NOT choose their own reward multiplier, work value or admission strength.
  - Growth in eligibility MUST come from verified protocol events.
  - Usage MUST NOT change the value of one unit of canonical work.
  - A new model MUST NOT change the reward rate, difficulty, coefficients or economic position of an
    unrelated model.
  - Admission rules MUST stay generic.

## 1.2 What the protocol verifies, and what it deliberately does not

| The protocol verifies… | Where | The protocol does not verify… |
| --- | --- | --- |
| that the class was a registered one | 03 | whether the question was worth asking |
| that the inference actually ran, as committed | 04, 08, 09 | whether the answer was good |
| how much canonical work it was | 05 | who the user is |
| that the same work is not paid twice | 07, 11 | what the prompt said |
| that this inference held an eligibility ticket | 06, 11 | |

- **PALW-PR-1 (no judgement of content).** No validity rule MAY read the meaning of a user's prompt,
  the quality of an answer, or the identity of a user. A free-prompt commitment is validated only on:
  - its shape, its version and its network;
  - its sizes, bounded by the class and the ruleset;
  - the hashes of its prompt ids against their commitment;
  - its signature;
  - its privacy mode;
  - its work.

  (`palw_freeprompt_v3.rs` `validate_stateless_under_ruleset_v3`, whose refusals are all of these
  kinds.) Checks on an attempt's prompt (09 PALW-CT-7 J5, `PromptNotAnchored`) apply only to prompts
  the protocol itself derives from the anchor.

## 1.3 Execute now, settle later

- **PALW-PR-2 (order).** The commitment over prompt, model and parameters MUST be fixed before the
  beacon that decides its eligibility resolves (06 PALW-EL-1). A miner therefore cannot draw a ticket
  first and then spend it on the cheapest job.
- **PALW-PR-3 (the product does not wait).** Nothing in the reward path may delay the answer shown to
  the user. The claim, the receipt and settlement all happen after the answer *(node policy on the
  host; 11 PALW-FP-4)*.
- **PALW-PR-4 (ineligible work leaves no trace).** An inference that no beacon makes eligible creates no
  claim, no obligation and no reward. It was just a local inference.

## 1.4 How each principle is enforced

| Principle | Enforced by | Gaps (and where they are tracked) |
| --- | --- | --- |
| P1 | 11 PALW-FP-16 (loopback entrance); 14 PALW-ND-16 (host sandbox); 15 PALW-MK-9/10 (a membership is never remote inference) | Consensus cannot tell local inference from remote. P1 is kept by the product surface and by what the reward path does not pay for. Stated plainly, and not claimed further |
| P2 | PALW-PR-1; 11 PALW-FP-1 (user tokens, nothing appended) | none known |
| P3 | 06 PALW-EL-4/5 (the ticket is the execution); 11 PALW-FP-4 (one inference); 11 PALW-FP-10 (a derivation earns nothing unless it is the local inference) | none known |
| P4 | 05 PALW-WK-1…3, WK-7 (derived work, `PwuClaimNotDerived`); 05 PALW-WK-4 (no coefficient table); 04 PALW-EX-10 (`tile_len` never feeds accounting) | The P4 and P5 property tests (`palw_reward_properties_v1.rs`) run on the armed rules. ADR-0144 §5's STEP-leaf and `tile_len` exposure is closed wherever `palw_canonical_work` is armed (testnet-12: genesis) |
| P5 | 04 PALW-EX-2 (backends below the semantic boundary); 14 PALW-ND-14; 05 PALW-WK-3 | none known |
| P6 | 06 §6.1–§6.3 (the beacon, the single lottery, the work target); 11 PALW-FP-5 (one-shot quanta) | Synthetic Attempt work is still the main reward path and the beacon source. ADR-0144 §6 item 5 (shrink Attempt) is open. The live mix is recorded in `design/palw/principles.md` |
| P7 | 03 §3.2–§3.3 (listing, derived profiles, the lifecycle as budget: 50/100/1,000 ‰, drawn independence); 05 PALW-WK-9/12 (one work target; a share is a result) | The listing rules have no ADR (divergences.md row 1). The operator's T12-030 rule: tools admit what the chain admits |
| §3 | PALW-PR-1 | none known |
| §4 | PALW-PR-2 to PR-4; 06 PALW-EL-1 | none known |

## 1.5 Scope rules

- **PALW-PR-5 (PALW settles on its own).** PALW settlement MUST NOT depend on DNS finality, on BFT
  validators or on a beacon (07 PALW-LC-24). PALW terms (settlement anchor, settled, `Final`) are not
  DNS terms. The one coupling that stays is the DNS stake reorg gate, which can only refuse more
  (13 PALW-FC-10).
- **PALW-PR-6 (out of scope).** Remote inference markets, GPU rental and third-party serving MUST NOT be
  reward surfaces. Withdrawn on those grounds (ADR-0144 §9):
  - ADR-0073 D2 (self-prompts) and Phase ④;
  - ADR-0077's public entrance;
  - ADR-0078 D7;
  - ADR-0079's Done-when;
  - ADR-0101 D4, D6 and D7.
- **PALW-PR-7 (do not widen the economy).** While ADR-0144 §6 items 2–3 are open, no fence may extend
  reward or admission beyond what this spec states. The deferred widenings are:
  - BPS above 1 (12 PALW-XL-6);
  - a scalar CCU (05 PALW-WK-4);
  - ADR-0073 Phase ④;
  - the DA reward.

**Sources:** ADR-0144 §2–§4, §6, §9; ADR-0127. **Code:** `core/palw_freeprompt_v3.rs`,
`core/palw_reward_properties_v1.rs`, `core/palw_settlement_v1.rs`.
