# ADR-0096 — The app you already use is the entrance, and the shape of the answer is committed

> **Body moved (2026-09-27).** Normative rules → [spec/palw/11](../spec/palw/11-free-prompt-lane.md), [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md](../design/palw/archive/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md); the reasoning is summarised in [design/palw/free-prompt.md](../design/palw/free-prompt.md).

* Status: Proposed 2026-09-10.
  - Part A (the entrance) and Part C (distribution, settings, the model-request intake) are host-side
    and landed as built.
  - Part B's consensus half (D6–D8: the committed answer shape and the decode constraint) sits behind
    `palw_fp_decode_constraint`, which is **dormant on testnet-12** (refused by validation; this build
    cannot carry it).
* Date: 2026-09-10

## Context

Everything a person does with a small local model was either refused by the free-prompt lane or
unrecorded: point the app they already use at it, keep their conversations, ask for JSON, let it call
a tool, carry a long thread.

## Decision

- **Part A — the entrance (host):**
  - D1: one OpenAI surface, served twice;
  - D2: a tool call is a turn of text;
  - D3: `response_format` in two enforcement modes;
  - D4: sampling mapped with notice;
  - D5: a long thread is a chain of jobs.
  - → spec 11 PALW-FP-16.
- **Part B — the answer's shape is committed:**
  - D6: the answer is cut at the first committed end-of-generation id;
  - D7: a job may carry a decode constraint;
  - D8: one fence, refused at assembly until the build carries all three halves;
  - D9: the `json` kind.
- **Part C — distribution:** D10–D13, the components manifest, settings, conversations, the
  model-request door.

## Consequences

- The app you already use is the entrance. The commitment of its answer's shape waits for the fence.

## Links

- Spec: [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md) · [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/free-prompt.md](../design/palw/free-prompt.md)
- Full text as written: [design/palw/archive/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md](../design/palw/archive/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
