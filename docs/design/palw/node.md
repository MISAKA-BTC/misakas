# Node duties, the operator and the host — design

> **Not normative.** This document explains why the rules in
> [spec/palw/14-node-duties.md](../../spec/palw/14-node-duties.md) are what they are.

**Decisions recorded in:** ADR-0057, 0079, 0097, 0106, 0108, 0112, 0121, 0122, 0136, and ADR-0152
Q-7 and P2 (the node's roles).
**Last revised:** 2026-09-27

## 1. Problem

Much of PALW's safety rests on nodes doing their duty. Seats must file, producers must disclose, and
someone must answer a court. None of that is enforced by block validity. And a local LLM host runs
strangers' models next to the operator's keys.

## 2. The design in one paragraph

- **Duties are always on:** decided by params and identity, never by a flag.
- **The seat's code paths are narrowed** so that only a real replay can sign `Valid` (SEAT-R). F2
  would otherwise slash honest seats whose material arms signed without checking arithmetic.
- **The host is sandboxed:** the worker starts with nothing and holds no key, and output is data.
- **Resource use is the operator's choice:** a stated read budget, one mapped copy, streamed
  inventories. It never touches the class's identity.
- **The operator sees purposes,** one work id, and one line saying why nothing is happening.

## Source texts (archived ADR bodies)

- [ADR-0122 — Mining is a purpose: an operator runs one command and reads one work id](archive/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md)
- [ADR-0079: A pure function needs no permissions — the sandbox is for the host, and the chain never takes its word for it](archive/0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md)
- [ADR-0097 — A model's fit is a lookup, and the entrance says its limits before the first token](archive/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
- [ADR-0112 — A class's weights are read within a budget the operator states, and the budget is a fifth of the artifact](archive/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md)
- [ADR-0106 — An inventory is a stream of leaves, not a copy of the model](archive/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md)
- [ADR-0108 — An extension is a manifest the verifier recomputes, and a receipt is evidence, not a vote](archive/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md)
