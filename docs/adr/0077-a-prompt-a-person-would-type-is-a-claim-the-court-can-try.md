# ADR-0077: A prompt a person would type is a claim the court can try

> **Body moved (2026-09-27).** Normative rules → [spec/palw/11](../spec/palw/11-free-prompt-lane.md), [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md](../design/palw/archive/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md); the reasoning is summarised in [design/palw/free-prompt.md](../design/palw/free-prompt.md).

* Status: Proposed (2026-09-02) and governing for the local path.
  - **ADR-0144 alignment (2026-09-21):** R0, Phase A and local Decision 3 are kept as the product path.
    The public commercial entrance is withdrawn as a PALW-reward goal (P1).
  - Implementation note (2026-09-06): Decision 13 went into the context ladder.
  - Phase B's `palw_context_ladder` is a separate fence from the court's `palw_court_ladder` (README,
    "Two labels").
* Date: 2026-09-02

## Context

It was written against a measurement. The network's purpose is a person using their own local LLM on
their own prompt, with that one inference mining. At the time, no such prompt could be tried by the
court at a usable width.

## Decision

- **D1–D5 — The entrance:**
  - one runtime (the server is the worker);
  - the answer streams and the commitment does not;
  - the gateway reads the chain it commits to;
  - one handoff;
  - the family workers are the gateway's.
- **D6 — The template speaks the model's own control tokens,** segment-wise.
- **D7 — The drill is a chain, not a harness.**
- **D8 — The seat verifies one interval; the executor opens it.** → spec 11 PALW-FP-9.
- **D9 — The public pages say how.**
- **D10–D11 — History is replayed from a checkpoint,** and admission prices the checkpoint interval.
- **D12–D14 — The ladder is sized to an artifact,** the context ladder rows, and the canonical job grows
  with the context.
- **D15 — Weight follows measured supply.**
- **D16 — `PanelDa`, privacy mode 2.** → spec 11 PALW-FP-3.

## Consequences

- A prompt a person would type is a claim the court can try. The public-serving goal is out of scope.

## Links

- Spec: [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md) · [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/free-prompt.md](../design/palw/free-prompt.md)
- Full text as written: [design/palw/archive/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md](../design/palw/archive/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)
