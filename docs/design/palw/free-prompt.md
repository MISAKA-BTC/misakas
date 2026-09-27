# The free-prompt lane — design

> **Not normative.** This document explains why the rules in
> [spec/palw/11-free-prompt-lane.md](../../spec/palw/11-free-prompt-lane.md) are what they are.

**Decisions recorded in:** ADR-0044, 0073, 0077, 0078, 0084, 0096, 0145 §6, 0148, and ADR-0144
(P1–P3 above all). **Proposal pending:** RFC-0001 (FP Job V4, on `rcore/fp-sampler`).
**Last revised:** 2026-09-27

## 1. Problem

The constitution's sentence (ADR-0144): a person uses a local LLM on their own machine, with prompts
they chose, and the same inference that answered them is the one the chain rewards. The measurement
of 2026-09-19 showed this lane carried almost none of the economy. Nine `FreePromptCommitted` objects
in four days, against a block mix that was entirely synthetic attempts.

## 2. The design in one paragraph

The user's tokens are committed before any beacon can make them eligible. The answer streams to the
user at once, and the chain settles later. Eligibility comes in one-shot quanta that a later beacon
draws, priced by the same derived compute as an attempt, over new positions only, so a cached prefix
earns nothing twice. A claim serves its answer, never its history. Seats verify one interval, opened
by the executor. The entrance is the user's own machine. A public gateway selling someone else's GPU
is not a reward surface.

## 3. What remains open

- The richer decode rules (ADR-0082 D10/D11) and the decode constraint (ADR-0096 D6–D8) are dormant
  on testnet-12. RFC-0001 §A is the frozen proposal for a deterministic decode pipeline
  (`DecodeConfigV4`).
- ADR-0073 Phase ④ is withheld until ADR-0144 §6 items 2–3 land.
- Shrinking synthetic Attempt (ADR-0144 §6 item 5) is what would make this lane the economy.

## Source texts (archived ADR bodies)

- [ADR-0096 — The app you already use is the entrance, and the shape of the answer is committed](archive/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
- [ADR-0077: A prompt a person would type is a claim the court can try](archive/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)
- [ADR-0078: What was made from it is committed; the thing itself never rides](archive/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md)
- [ADR-0084: The ids ride, the capture stays home — a model-class claim serves its answer, never its history](archive/0084-the-ids-ride-the-capture-stays-home.md)
- [ADR-0073: Real-demand work bears the weight](archive/0073-real-demand-work-bears-the-weight.md)
- [ADR-0044: Free-prompt PALW — the user's own inference becomes the consensus work, certified before it mines](archive/0044-palw-free-prompt-receipts.md)
