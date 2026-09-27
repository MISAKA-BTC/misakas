# ADR-0079: A pure function needs no permissions — the sandbox is for the host, and the chain never takes its word for it

> **Body moved (2026-09-27).** Normative rules → [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md](../design/palw/archive/0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md); the reasoning is summarised in [design/palw/node.md](../design/palw/node.md).

* Status: Proposed (2026-09-02). **ADR-0144 alignment (2026-09-21):** the sandbox stays for the loopback
  host. Its "Done when" (a public LLM entrance) is withdrawn as a PALW product. The security amendment
  of 2026-09-02 is in the full text.
* Date: 2026-09-02

## Context

A proposal said ADR-0078 was not enough to run a local LLM in practice, and that ten provenance layers
and five security ADRs were missing beneath it. The reading was right about the host and wrong about
the chain: determinism already sets the permissions of a pure function.

## Decision

- **D1 — The capability set is the arithmetic's,** deny-by-default.
- **D2 — No security field enters the priced bytes,** in any lane, ever.
- **D3 — The security posture is off the consensus path,** and a test proves it cannot fork the chain.
- **D4 — No process that parses a stranger's bytes holds a key.**
- **D5 — The worker starts with nothing** and is confined by the platform.
- **D6 — Every job has a memory ceiling and a wall-clock deadline.**
- **D7 — Untrusted text cannot become a control token.**
- **D8 — A model's output is data on every path.**
- **D9 — Model and runtime integrity is a full read,** permanently.
- **D10 — The public entrance is bounded,** and it is never the seat. *Withdrawn as a PALW goal.*
- **D11 — A stranger's graph stays behind ADR-0067 D5's fence.**
- **D12 — An external toolchain is the largest privilege.**
- **D13 — The posture is a local report,** signed by nobody.

→ spec 14 PALW-ND-16.

## Consequences

- The host is safe to run for its user. The chain never takes the host's word for anything.

## Links

- Spec: [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/node.md](../design/palw/node.md)
- Full text as written: [design/palw/archive/0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md](../design/palw/archive/0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md)
