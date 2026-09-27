# ADR-0144 — PALW pays for the inference you were going to run anyway

> **Body moved (2026-09-27).** Normative rules → [spec/palw/01](../spec/palw/01-principles.md); the full text as written → [design/palw/archive/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md](../design/palw/archive/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md); the reasoning is summarised in [design/palw/principles.md](../design/palw/principles.md).

* Status: **Constitution, 2026-09-19.** It fixes what PALW is for. It changes no code and arms no fence.
  A proposal is in scope if it serves the sentence below, and out of scope if it does not.
  Alignment pass: 2026-09-21 (§9 of the full text; README "What to build next").
* Date: 2026-09-19

> **A person uses a local LLM on their own machine, with prompts they chose for their own reasons,
> and the same inference that answered them is the inference the chain rewards.**

## Context

The mechanism already existed: `palw_freeprompt_v3.rs` calls the free-prompt job "the data layer of
'the user's own inference becomes the consensus work'". But the economy did not follow. On testnet-11
between 2026-09-15 and 2026-09-19 there were nine `FreePromptCommitted` objects, against a block mix
that was 100 % synthetic attempts. Whatever is cheapest to farm is what the economy does.

## Decision

- **The principles:**
  - **P1** local first;
  - **P2** free prompt;
  - **P3** same inference, one purpose;
  - **P4** representation-neutral accounting;
  - **P5** efficiency is rewarded and accounting tricks are not;
  - **P6** local inference is unlimited, while reward eligibility is scarce and protocol-assigned;
  - **P7** model admission is permissionless, and economic weight is earned through verified use
    (budget, never price).

  → spec 01 §1.1.
- **§3 — What the protocol verifies,** and what it deliberately does not. → spec 01 §1.2.
- **§4 — Execute now, settle later:** the beacon resolves after execution began. → spec 01 §1.3.
- **§5 — The unit of canonical work is not decided here.** A vector is the direction (answered by
  ADR-0145/0146).
- **§6 — Order of work:**
  0. do not widen the economy;
  1. this ADR;
  2. the accounting redesign;
  3. the admission redesign;
  4. local FreePrompt as the standard path;
  5. shrink synthetic Attempt.

## Consequences

- Earlier unimplemented ADRs that contradicted the constitution were amended in place (0069, 0073, 0075,
  0077, 0078, 0079, 0101, 0130, 0131).
- Success is not a green test suite. It is a person using the Studio for a day without changing a
  prompt for mining, and some of that inference earning (§8).

## Links

- Spec: [01 Principles and scope](../spec/palw/01-principles.md)
- Design: [design/palw/principles.md](../design/palw/principles.md)
- Full text as written: [design/palw/archive/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md](../design/palw/archive/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)
