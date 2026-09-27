# ADR-0108 — An extension is a manifest the verifier recomputes, and a receipt is evidence, not a vote

> **Body moved (2026-09-27).** Normative rules → [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md](../design/palw/archive/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md); the reasoning is summarised in [design/palw/node.md](../design/palw/node.md).

* Status: Proposed 2026-09-11 at the operator's request. **D1–D9 implemented, consensus-inert:** no
  object, rule, fence, parameter or fingerprint moves.
* Date: 2026-09-11

## Context

The operator asked for a permissionless foundation for extensions. People make things with a model, a
converter, a drill, or by hand. Each should be describable once, and recomputable by any node's
verifier.

## Decision

- **D1 — One manifest,** canonical (RFC 8785), with a derived identity (`PalwExtensionManifestV1`).
- **D2 — Every answer names its tier.**
- **D3 — Verification has three depths,** and the report says which it reached.
- **D4 — A receipt is evidence of reproduction,** signed by whoever reproduced it. It is not a vote.
- **D5 — Preflight and submit go through the objects that already exist.**
- **D6 — A ruleset candidate is described and costed,** never activated by a manifest.
- **D7 — A context width is a class inside the ladder,** and a ruleset candidate outside it.
- **D8 — The chain-class seal stays.**
- **D9 — Publication and discovery are off chain,** and the id is what makes that safe.

→ spec 14 PALW-ND-17.

## Consequences

- Extensions are recomputable claims about artifacts. They never change consensus by themselves.

## Links

- Spec: [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/node.md](../design/palw/node.md)
- Full text as written: [design/palw/archive/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md](../design/palw/archive/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md)
