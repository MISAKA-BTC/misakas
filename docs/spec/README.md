# Specification chapters

Normative PALW chapters live in [`palw/`](palw/):

| file | what it specifies |
|---|---|
| [`04b-tensor-ir.md`](palw/04b-tensor-ir.md) | PALW-TIR (RFC-0002): the IR, class registration, class seating |
| [`17-model-improvement.md`](palw/17-model-improvement.md) | RFC-0003/0004 (generative classes, model improvement), and §17.0 — the object-tag, delta and tail tables |
| [`18-inference-surface.md`](palw/18-inference-surface.md) | RFC-0001: the inference-surface rules |
| [`18-layer-sharded-panels.md`](palw/18-layer-sharded-panels.md) | RFC-0006: layer-sharded panels |
| [`18-verification-certificates.md`](palw/18-verification-certificates.md) | RFC-0007: verification vertices, the audit mesh, capped onboarding |

**Numbering note (2026-10-03).** The three files that begin `18-` are three distinct chapters that were written in parallel and
share the number by accident. They will be renumbered **18 (inference surface), 19 (layer-sharded panels) and 20 (verification
certificates)** — with every "spec 18" reference — after the DAA-5,300 flag day is deployed; until then "spec 18" in a comment or
a document means the chapter of the RFC it sits beside (RFC-0001, RFC-0006 or RFC-0007 respectively).
