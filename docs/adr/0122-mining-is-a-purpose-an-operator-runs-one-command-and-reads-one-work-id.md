# ADR-0122 — Mining is a purpose: an operator runs one command and reads one work id

> **Body moved (2026-09-27).** Normative rules → [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md](../design/palw/archive/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md); the reasoning is summarised in [design/palw/node.md](../design/palw/node.md).

* Status: Proposed 2026-09-12. **Consensus-inert:** it adds the CLI, three additive RPC reads and
  additive log lines, and no object, rule, fence or fingerprint moves. Partly built. The `misaka` verbs
  exist and grew after launch (the launch note §00 CLI fixes).
* Date: 2026-09-12

## Context

The operator asked to turn PALW mining from a set of components into a product.

## Decision

- **D1 — The verbs are purposes;** components are a developer's view.
- **D2 — The work state machine, and its one id.**
- **D3 — The miner's own state,** and the one line that says why nothing is happening.
- **D4 — Human errors:** one catalogue, five fields.
- **D5 — One command to start,** a safe stop, and the readiness gates.
- **D6 — One configuration file,** purpose first.
- **D7 — Setup:** resumable, and it discovers instead of asking.
- **D8 — Structured events:** new lines, with old lines untouched.
- **D9 — The dashboard.**
- **D10 — The order of work.**

→ spec 14 PALW-ND-18.

## Consequences

- The operator reads one work id from request to reward.

## Links

- Spec: [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/node.md](../design/palw/node.md)
- Full text as written: [design/palw/archive/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md](../design/palw/archive/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md)
